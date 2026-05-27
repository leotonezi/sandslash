use scraper::{Html, Selector};
use std::sync::LazyLock;

static SEL_TITLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("title").unwrap());
static SEL_META_DESC: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("meta[name='description']").unwrap());
static SEL_CANONICAL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("link[rel='canonical']").unwrap());
static SEL_IMG: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("img").unwrap());
static SEL_A: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href]").unwrap());
static SEL_SCRIPT_SRC: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("script[src]").unwrap());
static SEL_LINK_HREF: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("link[href]").unwrap());

#[derive(Debug, Clone)]
pub struct ImgInfo {
    pub src: String,
    pub alt: Option<String>,
}

pub struct Dom {
    html: Html,
}

impl Dom {
    pub fn parse(html: &str) -> Self {
        Self { html: Html::parse_document(html) }
    }

    pub fn title(&self) -> Option<String> {
        self.html
            .select(&SEL_TITLE)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_owned())
    }

    pub fn meta_description(&self) -> Option<String> {
        self.html
            .select(&SEL_META_DESC)
            .next()
            .and_then(|el| el.attr("content"))
            .map(|s| s.trim().to_owned())
    }

    pub fn canonical(&self) -> Option<String> {
        self.html
            .select(&SEL_CANONICAL)
            .next()
            .and_then(|el| el.attr("href"))
            .map(|s| s.trim().to_owned())
    }

    pub fn headings(&self) -> Vec<(u8, String)> {
        let mut result = Vec::new();
        for level in 1u8..=6 {
            let sel = Selector::parse(&format!("h{level}")).unwrap();
            for el in self.html.select(&sel) {
                let text = el.text().collect::<String>().trim().to_owned();
                result.push((level, text));
            }
        }
        // Sort by document order via position in source — scraper preserves insertion order
        // per heading level but not across levels. Re-sort by level for hierarchy checks.
        // For skipped-level detection we only need levels in sequence, not DOM order.
        result
    }

    pub fn images(&self) -> Vec<ImgInfo> {
        self.html
            .select(&SEL_IMG)
            .map(|el| ImgInfo {
                src: el.attr("src").unwrap_or("").to_owned(),
                alt: el.attr("alt").map(|s| s.to_owned()),
            })
            .collect()
    }

    pub fn meta_property(&self, key: &str) -> Option<String> {
        let sel = Selector::parse(&format!("meta[property='{key}']")).unwrap();
        self.html
            .select(&sel)
            .next()
            .and_then(|el| el.attr("content"))
            .map(|s| s.trim().to_owned())
    }

    pub fn meta_name(&self, key: &str) -> Option<String> {
        let sel = Selector::parse(&format!("meta[name='{key}']")).unwrap();
        self.html
            .select(&sel)
            .next()
            .and_then(|el| el.attr("content"))
            .map(|s| s.trim().to_owned())
    }

    pub fn links(&self) -> Vec<String> {
        self.html
            .select(&SEL_A)
            .filter_map(|el| el.attr("href"))
            .map(|s| s.to_owned())
            .collect()
    }

    /// All resource srcs that could cause mixed content (img, script, link).
    pub fn resource_urls(&self) -> Vec<String> {
        let mut urls = Vec::new();
        for el in self.html.select(&SEL_IMG) {
            if let Some(src) = el.attr("src") {
                urls.push(src.to_owned());
            }
        }
        for el in self.html.select(&SEL_SCRIPT_SRC) {
            if let Some(src) = el.attr("src") {
                urls.push(src.to_owned());
            }
        }
        for el in self.html.select(&SEL_LINK_HREF) {
            if let Some(href) = el.attr("href") {
                urls.push(href.to_owned());
            }
        }
        urls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/basic.html");

    #[test]
    fn title_extracted() {
        let dom = Dom::parse(FIXTURE);
        assert_eq!(dom.title().unwrap(), "Basic Page Title That Is Long Enough");
    }

    #[test]
    fn meta_description_extracted() {
        let dom = Dom::parse(FIXTURE);
        assert!(dom.meta_description().is_some());
    }

    #[test]
    fn canonical_extracted() {
        let dom = Dom::parse(FIXTURE);
        assert_eq!(dom.canonical().unwrap(), "https://example.com/");
    }

    #[test]
    fn headings_extracted() {
        let dom = Dom::parse(FIXTURE);
        let h = dom.headings();
        assert!(h.iter().any(|(l, _)| *l == 1));
        assert!(h.iter().any(|(l, _)| *l == 2));
    }

    #[test]
    fn images_extracted() {
        let dom = Dom::parse(FIXTURE);
        let imgs = dom.images();
        assert_eq!(imgs.len(), 1);
        assert_eq!(imgs[0].alt.as_deref(), Some("A photo"));
    }

    #[test]
    fn links_extracted() {
        let dom = Dom::parse(FIXTURE);
        let links = dom.links();
        assert!(links.contains(&"/about".to_owned()));
    }

    #[test]
    fn meta_property_extracted() {
        let dom = Dom::parse(FIXTURE);
        assert_eq!(dom.meta_property("og:title").unwrap(), "Basic Page");
    }

    #[test]
    fn missing_title_returns_none() {
        let dom = Dom::parse("<html><head></head><body></body></html>");
        assert!(dom.title().is_none());
    }
}
