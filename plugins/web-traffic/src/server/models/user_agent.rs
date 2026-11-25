//! What a user-agent string says about a visitor, in the few words a traffic
//! report needs. Bots are told apart so they never count as visitors.

#[derive(Debug, PartialEq, Eq)]
pub struct Agent {
    pub browser: &'static str,
    pub os: &'static str,
    pub device: &'static str,
}

const BOTS: &[&str] = &[
    "bot",
    "crawl",
    "spider",
    "slurp",
    "headless",
    "lighthouse",
    "pingdom",
    "monitor",
    "curl/",
    "wget/",
    "python-requests",
    "httpclient",
    "preview",
    "facebookexternalhit",
    "embedly",
];

/// `None` for a bot, or for no user agent at all.
pub fn parse(ua: &str) -> Option<Agent> {
    let lower = ua.to_ascii_lowercase();
    if lower.is_empty() || BOTS.iter().any(|b| lower.contains(b)) {
        return None;
    }
    // Order matters: Edge and Opera say Chrome too, Chrome says Safari too.
    let browser = if ["edg/", "edga/", "edgios/"]
        .iter()
        .any(|e| lower.contains(e))
    {
        "Edge"
    } else if lower.contains("opr/") || lower.contains("opera") {
        "Opera"
    } else if lower.contains("samsungbrowser") {
        "Samsung Internet"
    } else if lower.contains("firefox/") || lower.contains("fxios/") {
        "Firefox"
    } else if lower.contains("chrome/") || lower.contains("crios/") {
        "Chrome"
    } else if lower.contains("safari/") {
        "Safari"
    } else {
        "Other"
    };
    let os = if ["iphone", "ipad", "ipod"].iter().any(|d| lower.contains(d)) {
        "iOS"
    } else if lower.contains("android") {
        "Android"
    } else if lower.contains("windows") {
        "Windows"
    } else if lower.contains("mac os x") || lower.contains("macintosh") {
        "macOS"
    } else if lower.contains("cros") {
        "ChromeOS"
    } else if lower.contains("linux") {
        "Linux"
    } else {
        "Other"
    };
    let device = if lower.contains("ipad")
        || lower.contains("tablet")
        || (os == "Android" && !lower.contains("mobile"))
    {
        "Tablet"
    } else if lower.contains("mobi") || lower.contains("iphone") {
        "Mobile"
    } else {
        "Desktop"
    };
    Some(Agent {
        browser,
        os,
        device,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_browsers_apart_even_when_they_claim_each_other() {
        let chrome = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";
        let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36 Edg/126.0";
        let safari_iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
        let firefox_mac =
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 14.5; rv:127.0) Gecko/20100101 Firefox/127.0";
        let android_tablet = "Mozilla/5.0 (Linux; Android 14; SM-X710) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";
        assert_eq!(
            parse(chrome),
            Some(Agent {
                browser: "Chrome",
                os: "Windows",
                device: "Desktop"
            })
        );
        assert_eq!(parse(edge).unwrap().browser, "Edge");
        assert_eq!(
            parse(safari_iphone),
            Some(Agent {
                browser: "Safari",
                os: "iOS",
                device: "Mobile"
            })
        );
        assert_eq!(
            parse(firefox_mac),
            Some(Agent {
                browser: "Firefox",
                os: "macOS",
                device: "Desktop"
            })
        );
        assert_eq!(parse(android_tablet).unwrap().device, "Tablet");
    }

    #[test]
    fn bots_are_not_visitors() {
        for ua in [
            "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/126.0 Safari/537.36",
            "curl/8.6.0",
            "",
        ] {
            assert_eq!(parse(ua), None, "{ua}");
        }
    }
}
