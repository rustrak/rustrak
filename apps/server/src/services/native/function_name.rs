//! Sentry's `trim_function_name` (`src/sentry/stacktraces/functions.py`,
//! 2458718b2c): the short `function` Sentry shows for a symbolicated frame,
//! keeping the full name as `raw_function`.
//!
//! A line-by-line port, including Python's quirks (character indices,
//! negative indexing), so the same symbol trims to the same name.

use std::sync::LazyLock;

use fancy_regex::Regex;

use super::NATIVE_PLATFORMS;

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

static WINDECL_HASH: LazyLock<Regex> = LazyLock::new(|| regex(r"^@?(.*?)@[0-9]+$"));
static RUST_HASH: LazyLock<Regex> = LazyLock::new(|| regex(r"::h[a-z0-9]{16}$"));
/// The 40-character hash IL2CPP appends to managed method names, kept off
/// the descriptor suffixes that name distinct generated functions.
static IL2CPP_METHOD_HASH: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"_m[0-9a-fA-F]{40}(?=(_gshared|_fshared|_inline|_AdjustorThunk|_Multicast|_Open(Static|Instance|Virtual|Interface|GenericVirtual|GenericInterface)(Invoker)?|_gp(_\d+)+)*$)",
    )
});
static GNU_VERSION: LazyLock<Regex> = LazyLock::new(|| regex(r"@@?GLIBC_([0-9.]+)$"));
static CPP_TRAILER: LazyLock<Regex> = LazyLock::new(|| regex(r"(\bconst\b|&)$"));
static RUST_BLANKET: LazyLock<Regex> = LazyLock::new(|| regex(r"^([A-Z] as )"));
/// gcc `{lambda(..)#N}`, msvc `lambda_<hash>`, clang `$_N`.
static LAMBDA: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?:\{lambda\(.*?\)\#\d+\})|(?:\blambda_[a-f0-9]{32}\b)|(?:\$_\d+\b)"));
static ANON_NAMESPACE: LazyLock<Regex> = LazyLock::new(|| regex(r"\?A0x[a-f0-9]{8}::"));
static FUNCTION_TOKEN_BREAK: LazyLock<Regex> = LazyLock::new(|| regex(r"\s|[({<\[]"));

fn sub(re: &Regex, replacement: &str, text: &str) -> String {
    re.replace_all(text, replacement).into_owned()
}

fn is_match(re: &Regex, text: &str) -> bool {
    re.is_match(text).unwrap_or(false)
}

fn closing(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '{' => Some('}'),
        '[' => Some(']'),
        '<' => Some('>'),
        _ => None,
    }
}

/// Sentry's `replace_enclosed_string`: each outermost `start..end` span is
/// replaced by what `replacement` returns for its contents and its start
/// (in characters).
pub fn replace_enclosed_string(
    s: &str,
    start: char,
    end: char,
    replacement: &mut dyn FnMut(&str, usize) -> String,
) -> String {
    if !s.contains(start) {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut depth: i64 = 0;
    let mut out = String::new();
    let mut pair_start = 0;
    for (idx, &c) in chars.iter().enumerate() {
        if c == start {
            if depth == 0 {
                pair_start = idx;
            }
            depth += 1;
        } else if c == end {
            depth -= 1;
            if depth == 0 {
                let inner: String = chars[pair_start + 1..idx].iter().collect();
                out.push_str(&replacement(&inner, pair_start));
            }
        } else if depth == 0 {
            out.push(c);
        }
    }
    out
}

/// Sentry's `split_func_tokens`: whitespace-separated tokens, keeping
/// bracketed groups whole.
pub fn split_func_tokens(s: &str) -> Vec<String> {
    if !s.is_empty() && !is_match(&FUNCTION_TOKEN_BREAK, s) {
        return vec![s.to_string()];
    }
    let chars: Vec<char> = s.chars().collect();
    let slice = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    let mut buf: Vec<String> = Vec::new();
    let mut rv: Vec<Vec<String>> = Vec::new();
    let mut stack: Vec<char> = Vec::new();
    let mut end = 0;
    for (idx, &c) in chars.iter().enumerate() {
        if let Some(close) = closing(c) {
            stack.push(close);
        } else if stack.last() == Some(&c) {
            stack.pop();
            if stack.is_empty() {
                buf.push(slice(end, idx + 1));
                end = idx + 1;
            }
        } else if stack.is_empty() {
            if c.is_whitespace() {
                if !buf.is_empty() {
                    rv.push(std::mem::take(&mut buf));
                }
            } else {
                buf.push(slice(end, idx + 1));
            }
            end = idx + 1;
        }
    }
    if !buf.is_empty() {
        rv.push(buf);
    }
    rv.into_iter().map(|parts| parts.concat()).collect()
}

/// Python's `seq[i]`, negative indices counting from the end.
fn py_index(tokens: &[String], i: isize) -> Option<&String> {
    let len = tokens.len() as isize;
    let i = if i < 0 { len + i } else { i };
    (0..len).contains(&i).then(|| &tokens[i as usize])
}

/// Sentry's `trim_function_name` for `platform`.
pub fn trim_function_name(function: &str, platform: &str) -> String {
    if platform == "csharp" {
        return function.split(" (").next().unwrap_or(function).to_string();
    }
    if NATIVE_PLATFORMS.contains(&platform) {
        return trim_native_function_name(function, platform);
    }
    function.to_string()
}

fn trim_native_function_name(function: &str, platform: &str) -> String {
    if function == "<redacted>" || function == "<unknown>" {
        return function.to_string();
    }
    let original = function;
    let mut function = function.trim().to_string();

    // Objective-C selectors stay as they are.
    if function.starts_with('[') || function.starts_with("+[") || function.starts_with("-[") {
        return function;
    }

    // `[clone .foo]` annotations of cloned/split functions go.
    function = replace_enclosed_string(&function, '[', ']', &mut |value, _| {
        if value.starts_with("clone .") {
            String::new()
        } else {
            format!("[{value}]")
        }
    })
    .trim_end()
    .to_string();

    // C++ trailers.
    while let Ok(Some(m)) = CPP_TRAILER.find(&function) {
        function = function[..m.start()].trim_end().to_string();
    }

    // `operator<<` and friends would unbalance the brackets below.
    function = function
        .replace("operator<<", "operator⟨⟨")
        .replace("operator<", "operator⟨")
        .replace("operator()", "operator◯")
        .replace("operator->", "operator⟿")
        .replace(" -> ", " ⟿ ")
        .replace("`anonymous namespace'", "〔anonymousnamespace〕");

    if function.contains("lambda") || function.contains("$_") {
        function = sub(&LAMBDA, "lambda", &function);
    }
    if function.contains("?A0x") {
        function = sub(&ANON_NAMESPACE, "〔anonymousnamespace〕::", &function);
    }

    // The argument list goes.
    function = replace_enclosed_string(&function, '(', ')', &mut |value, _| {
        let value = value.trim();
        if value == "anonymous namespace" || value == "operator" {
            format!("({value})")
        } else {
            String::new()
        }
    });

    function = sub(&IL2CPP_METHOD_HASH, "", &function);

    // Generics become `<T>`, except Rust's `<Foo as Bar>::baz` trait form.
    fn process_generics(value: &str, start: usize) -> String {
        if value == "lambda"
            || LAMBDA
                .find(value)
                .ok()
                .flatten()
                .is_some_and(|m| m.start() == 0)
        {
            return format!("<{value}>");
        }
        if start > 0 {
            return "<T>".to_string();
        }
        let value = sub(&RUST_BLANKET, "", value);
        let value = replace_enclosed_string(&value, '<', '>', &mut process_generics);
        value.split(" as ").next().unwrap_or(&value).to_string()
    }
    function = replace_enclosed_string(&function, '<', '>', &mut process_generics);

    let is_thunk = function.contains("thunk for "); // Swift

    let mut tokens = split_func_tokens(&function);

    // MSVC puts a space between a generic operator and its generics.
    if tokens.len() > 1 && tokens.last().is_some_and(|t| t == "<T>") {
        tokens.pop();
        if let Some(last) = tokens.last_mut() {
            last.push_str(" <T>");
        }
    }

    // The function name is the token before the last `->` (Swift), or else
    // the last token.
    let func_token: Option<String> = if function.contains("initialization expression of") {
        tokens.iter().rposition(|t| t == "of").and_then(|of| {
            let mut token = tokens.get(of + 1)?.clone();
            if token == "static" {
                token = tokens.get(of + 2)?.clone();
            }
            let token = token.strip_prefix(':').unwrap_or(&token).to_string();
            Some(format!("initializer expression of {token}"))
        })
    } else if !tokens.is_empty() {
        match tokens.iter().rposition(|t| t == "⟿") {
            Some(arrow) => {
                let arrow = arrow as isize;
                py_index(&tokens, arrow - 1).and_then(|token| {
                    if token == "throws" {
                        py_index(&tokens, arrow - 2).cloned()
                    } else {
                        Some(token.clone())
                    }
                })
            }
            None => tokens.last().cloned(),
        }
        // Python falls back to the last token on an IndexError too.
        .or_else(|| tokens.last().cloned())
    } else {
        None
    };

    let mut function = match func_token.filter(|t| !t.is_empty()) {
        Some(token) => {
            let token = if token.starts_with('@') && (platform == "cocoa" || platform == "swift") {
                // A Swift attribute, not a name: an anonymous function.
                format!("{}closure", if is_thunk { "thunk for " } else { "" })
            } else if tokens.iter().any(|t| t == "closure") && token != "closure" {
                format!("closure in {token}")
            } else {
                token
            };
            token
                .replace('⟨', "<")
                .replace('◯', "()")
                .replace('⟿', "->")
                .replace("〔anonymousnamespace〕", "`anonymous namespace'")
        }
        None => original.to_string(),
    };

    if function.contains("::h") {
        function = sub(&RUST_HASH, "", &function);
    }
    if function.contains("@GLIBC_") {
        function = sub(&GNU_VERSION, "", &function);
    }
    if function.contains('@') {
        function = sub(&WINDECL_HASH, "$1", &function);
    }
    function
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enclosed_string_like_sentry() {
        // Sentry `test_enclosed_string_simple`.
        assert_eq!(
            replace_enclosed_string("foo::bar<Blah, Blah<Blah, Blah>>", '<', '>', &mut |_, _| {
                "<T>".into()
            }),
            "foo::bar<T>"
        );
        assert_eq!(
            replace_enclosed_string("foo::bar(unsigned int, int)", '(', ')', &mut |_, _| {
                String::new()
            }),
            "foo::bar"
        );
        assert_eq!(
            replace_enclosed_string(
                "(anonymous namespace)::foo::bar(unsigned int, int)",
                '(',
                ')',
                &mut |value, start| if start == 0 {
                    assert_eq!(value, "anonymous namespace");
                    "(new value)".into()
                } else {
                    "()".into()
                }
            ),
            "(new value)::foo::bar()"
        );
    }

    #[test]
    fn split_func_tokens_like_sentry() {
        // Sentry `test_split_func_tokens`.
        for (input, output) in [
            (
                "namespace::Class::function",
                vec!["namespace::Class::function"],
            ),
            ("foo bar baz", vec!["foo", "bar", "baz"]),
            (
                "foo bar (enclosed baz)",
                vec!["foo", "bar", "(enclosed baz)"],
            ),
            (
                "foo (enclosed bar) baz",
                vec!["foo", "(enclosed bar)", "baz"],
            ),
            (
                "foo(enclosed bar)baz {moar stuff}",
                vec!["foo(enclosed bar)baz", "{moar stuff}"],
            ),
            ("foo bar [baz (blah)]", vec!["foo", "bar", "[baz (blah)]"]),
        ] {
            assert_eq!(split_func_tokens(input), output, "{input}");
        }
    }
}
