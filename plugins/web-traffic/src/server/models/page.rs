//! What one page view says, from the page's URL, its referrer and the
//! request: the path, the campaign, where the visitor came from, and their
//! country and language. Pure, so every rule here is tested.

use url::Url;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub host: String,
    pub path: String,
    /// Kept only when the project keeps query strings.
    pub query: Option<String>,
    /// The site that sent the visitor; `None` when it was this one.
    pub referrer: Option<String>,
    pub utm_source: Option<String>,
    pub utm_medium: Option<String>,
    pub utm_campaign: Option<String>,
}

/// A page's URL as the report needs it. `None` for what is not a web page.
pub fn read(url: &str, referrer: Option<&str>, keep_query: bool) -> Option<Page> {
    let url = Url::parse(url).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = site(url.host_str()?);
    let param = |name: &str| {
        url.query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let referrer = referrer
        .and_then(|r| Url::parse(r).ok())
        .and_then(|r| r.host_str().map(site))
        .filter(|r| *r != host);
    Some(Page {
        path: shorten(url.path(), 500),
        query: url
            .query()
            .filter(|q| keep_query && !q.is_empty())
            .map(|q| shorten(q, 500)),
        // `ref=` is how links from newsletters and tools say where they come from.
        utm_source: param("utm_source").or_else(|| param("ref")),
        utm_medium: param("utm_medium"),
        utm_campaign: param("utm_campaign"),
        referrer,
        host,
    })
}

/// Whether `host` is one the project counts: any when none is set.
pub fn allowed(host: &str, domains: Option<&str>) -> bool {
    let Some(domains) = domains.map(str::trim).filter(|d| !d.is_empty()) else {
        return true;
    };
    domains
        .split(',')
        .map(|d| site(d.trim()))
        .filter(|d| !d.is_empty())
        .any(|d| host == d || host.ends_with(&format!(".{d}")))
}

/// The visitor's country: what the proxy in front says (Cloudflare, Vercel,
/// CloudFront), else the region of their language (`es-ES` is Spain).
pub fn country(header: impl Fn(&str) -> Option<String>, language: Option<&str>) -> Option<String> {
    [
        "cf-ipcountry",
        "x-vercel-ip-country",
        "cloudfront-viewer-country",
    ]
    .iter()
    .find_map(|h| header(h))
    .map(|c| c.trim().to_ascii_uppercase())
    .filter(|c| c.len() == 2 && c != "XX" && c != "T1")
    .or_else(|| {
        language?
            .split(['-', '_'])
            .nth(1)
            .filter(|r| r.len() == 2)
            .map(str::to_ascii_uppercase)
    })
}

/// A language tag as its primary language: `es-ES` is `es`.
pub fn language(tag: Option<&str>) -> Option<String> {
    tag?.split(['-', '_', ','])
        .next()
        .map(|l| l.trim().to_ascii_lowercase())
        .filter(|l| (2..=3).contains(&l.len()))
}

/// A host as one site: `www.Example.com` is `example.com`.
fn site(host: &str) -> String {
    host.trim_start_matches("www.").to_ascii_lowercase()
}

/// At most `max` characters, without the spaces around it.
pub fn shorten(s: &str, max: usize) -> String {
    s.trim().chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_path_the_campaign_and_where_the_visitor_came_from() {
        let page = read(
            "https://www.shop.example.com/pricing?utm_source=newsletter&utm_campaign=launch&plan=pro",
            Some("https://news.ycombinator.com/item?id=1"),
            false,
        )
        .unwrap();
        assert_eq!(page.host, "shop.example.com");
        assert_eq!(page.path, "/pricing");
        assert_eq!(page.query, None, "kept only when the project keeps queries");
        assert_eq!(page.utm_source.as_deref(), Some("newsletter"));
        assert_eq!(page.utm_campaign.as_deref(), Some("launch"));
        assert_eq!(page.referrer.as_deref(), Some("news.ycombinator.com"));
    }

    #[test]
    fn a_visit_from_the_same_site_has_no_referrer() {
        let page = read(
            "https://example.com/b",
            Some("https://www.example.com/a"),
            true,
        )
        .unwrap();
        assert_eq!(page.referrer, None);
        assert_eq!(page.query, None, "an empty query is none");
        let with_query = read("https://example.com/b?x=1", None, true).unwrap();
        assert_eq!(with_query.query.as_deref(), Some("x=1"));
    }

    #[test]
    fn only_web_pages_count() {
        assert_eq!(read("file:///etc/passwd", None, false), None);
        assert_eq!(read("not a url", None, false), None);
    }

    #[test]
    fn a_project_counts_its_domains_and_their_subdomains() {
        assert!(allowed("shop.example.com", None));
        assert!(allowed("shop.example.com", Some(" ")));
        assert!(allowed("example.com", Some("example.com, other.org")));
        assert!(allowed("shop.example.com", Some("www.example.com")));
        assert!(!allowed("example.com.evil.net", Some("example.com")));
        assert!(!allowed("evil.net", Some("example.com")));
    }

    #[test]
    fn the_country_comes_from_the_proxy_then_the_language() {
        let none = |_: &str| None;
        let cloudflare = |h: &str| (h == "cf-ipcountry").then(|| "es".to_string());
        assert_eq!(country(cloudflare, Some("en-US")).as_deref(), Some("ES"));
        assert_eq!(country(none, Some("pt-BR")).as_deref(), Some("BR"));
        assert_eq!(country(none, Some("fr")), None);
        let unknown = |h: &str| (h == "cf-ipcountry").then(|| "XX".to_string());
        assert_eq!(country(unknown, None), None);
        assert_eq!(language(Some("es-ES,es;q=0.9")).as_deref(), Some("es"));
    }
}
