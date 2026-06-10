use scraper::{Html, Selector, node::Node};
use std::sync::LazyLock;

static SEL_TITLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("title").expect("invariant: valid CSS selector"));
static SEL_META_DESC: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[name='description']").expect("invariant: valid CSS selector")
});
static SEL_CANONICAL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("link[rel='canonical']").expect("invariant: valid CSS selector")
});
static SEL_IMG: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("img").expect("invariant: valid CSS selector"));
#[allow(dead_code)]
static SEL_A: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href]").expect("invariant: valid CSS selector"));
static SEL_SCRIPT_SRC: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("script[src]").expect("invariant: valid CSS selector"));
static SEL_LINK_HREF: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("link[href]").expect("invariant: valid CSS selector"));
static SEL_IFRAME_SRC: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("iframe[src]").expect("invariant: valid CSS selector"));
static SEL_AUDIO_SRC: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("audio[src]").expect("invariant: valid CSS selector"));
static SEL_VIDEO_SRC: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("video[src]").expect("invariant: valid CSS selector"));
static SEL_SOURCE_SRC: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("source[src]").expect("invariant: valid CSS selector"));
static SEL_TRACK_SRC: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("track[src]").expect("invariant: valid CSS selector"));
static SEL_EMBED_SRC: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("embed[src]").expect("invariant: valid CSS selector"));
static SEL_OBJECT_DATA: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("object[data]").expect("invariant: valid CSS selector"));

/// Content-bearing semantic tags used for JS-rendered detection.
/// Covers: p, article, section, main, li, h1–h6.
static SEL_CONTENT_TAGS: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("p, article, section, main, li, h1, h2, h3, h4, h5, h6")
        .expect("invariant: valid CSS selector")
});

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ImgInfo {
    pub src: String,
    pub alt: Option<String>,
}

pub struct Dom {
    html: Html,
}

impl Dom {
    pub fn parse(html: &str) -> Self {
        Self {
            html: Html::parse_document(html),
        }
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

    /// All `<link rel="canonical">` hrefs in document order, trimmed.
    ///
    /// Returns an empty `Vec` when no canonical links are present.
    pub fn canonicals(&self) -> Vec<String> {
        self.html
            .select(&SEL_CANONICAL)
            .filter_map(|el| el.attr("href"))
            .map(|s| s.trim().to_owned())
            .collect()
    }

    pub fn headings(&self) -> Vec<(u8, String)> {
        let mut result = Vec::new();
        for level in 1u8..=6 {
            let sel = Selector::parse(&format!("h{level}"))
                .expect("invariant: h1–h6 are valid selectors");
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

    #[allow(dead_code)]
    pub fn images(&self) -> Vec<ImgInfo> {
        self.html
            .select(&SEL_IMG)
            .map(|el| ImgInfo {
                src: el.attr("src").unwrap_or("").to_owned(),
                alt: el.attr("alt").map(|s| s.to_owned()),
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn meta_property(&self, key: &str) -> Option<String> {
        let sel = Selector::parse(&format!("meta[property='{key}']")).ok()?;
        self.html
            .select(&sel)
            .next()
            .and_then(|el| el.attr("content"))
            .map(|s| s.trim().to_owned())
    }

    #[allow(dead_code)]
    pub fn meta_name(&self, key: &str) -> Option<String> {
        let sel = Selector::parse(&format!("meta[name='{key}']")).ok()?;
        self.html
            .select(&sel)
            .next()
            .and_then(|el| el.attr("content"))
            .map(|s| s.trim().to_owned())
    }

    #[allow(dead_code)]
    pub fn links(&self) -> Vec<String> {
        self.html
            .select(&SEL_A)
            .filter_map(|el| el.attr("href"))
            .map(|s| s.to_owned())
            .collect()
    }

    /// All resource URLs that could cause mixed content on an https page.
    ///
    /// Covers: img[src], script[src], link[href], iframe[src], audio[src],
    /// video[src], source[src], track[src], embed[src], object[data].
    pub fn resource_urls(&self) -> Vec<String> {
        let src_selectors: &[(&LazyLock<Selector>, &str)] = &[
            (&SEL_IMG, "src"),
            (&SEL_SCRIPT_SRC, "src"),
            (&SEL_IFRAME_SRC, "src"),
            (&SEL_AUDIO_SRC, "src"),
            (&SEL_VIDEO_SRC, "src"),
            (&SEL_SOURCE_SRC, "src"),
            (&SEL_TRACK_SRC, "src"),
            (&SEL_EMBED_SRC, "src"),
            (&SEL_OBJECT_DATA, "data"),
        ];

        let mut urls: Vec<String> = src_selectors
            .iter()
            .flat_map(|(sel, attr)| {
                self.html
                    .select(sel)
                    .filter_map(|el| el.attr(attr))
                    .map(|s| s.to_owned())
                    .collect::<Vec<_>>()
            })
            .collect();

        // link[href] uses a different attribute name — append separately
        urls.extend(
            self.html
                .select(&SEL_LINK_HREF)
                .filter_map(|el| el.attr("href"))
                .map(|s| s.to_owned()),
        );

        urls
    }

    /// Byte length of visible text in the document.
    ///
    /// Excludes text inside `<script>`, `<style>`, and `<noscript>` elements,
    /// as well as HTML comments. Used by JS-rendered detection.
    pub fn visible_text_len(&self) -> usize {
        // Names of elements whose text content is not user-visible.
        const EXCLUDED: &[&str] = &["script", "style", "noscript"];

        let mut total = 0usize;
        for node_ref in self.html.tree.nodes() {
            match node_ref.value() {
                Node::Text(text) => {
                    // Walk up the ancestor chain; skip if any ancestor is excluded.
                    let excluded = node_ref.ancestors().any(|ancestor| {
                        if let Node::Element(el) = ancestor.value() {
                            EXCLUDED.contains(&el.name())
                        } else {
                            false
                        }
                    });
                    if !excluded {
                        total += text.trim().len();
                    }
                }
                // Comments contribute no visible text — simply skip.
                Node::Comment(_) => {}
                _ => {}
            }
        }
        total
    }

    /// Number of semantic content tags present in the document.
    ///
    /// Counts: `p`, `article`, `section`, `main`, `li`, `h1`–`h6`.
    /// Used by JS-rendered detection.
    pub fn content_tag_count(&self) -> usize {
        self.html.select(&SEL_CONTENT_TAGS).count()
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

    #[test]
    fn resource_urls_includes_iframe_src() {
        let dom = Dom::parse(
            r#"<html><body><iframe src="http://iframe.example.com/frame"></iframe></body></html>"#,
        );
        assert!(
            dom.resource_urls()
                .contains(&"http://iframe.example.com/frame".to_owned())
        );
    }

    #[test]
    fn resource_urls_includes_audio_src() {
        let dom = Dom::parse(
            r#"<html><body><audio src="http://audio.example.com/track.mp3"></audio></body></html>"#,
        );
        assert!(
            dom.resource_urls()
                .contains(&"http://audio.example.com/track.mp3".to_owned())
        );
    }

    #[test]
    fn resource_urls_includes_video_src() {
        let dom = Dom::parse(
            r#"<html><body><video src="http://video.example.com/clip.mp4"></video></body></html>"#,
        );
        assert!(
            dom.resource_urls()
                .contains(&"http://video.example.com/clip.mp4".to_owned())
        );
    }

    #[test]
    fn resource_urls_includes_source_src() {
        let dom = Dom::parse(
            r#"<html><body><video><source src="http://source.example.com/clip.mp4"></video></body></html>"#,
        );
        assert!(
            dom.resource_urls()
                .contains(&"http://source.example.com/clip.mp4".to_owned())
        );
    }

    #[test]
    fn resource_urls_includes_track_src() {
        let dom = Dom::parse(
            r#"<html><body><video><track src="http://track.example.com/subs.vtt"></video></body></html>"#,
        );
        assert!(
            dom.resource_urls()
                .contains(&"http://track.example.com/subs.vtt".to_owned())
        );
    }

    #[test]
    fn resource_urls_includes_embed_src() {
        let dom = Dom::parse(
            r#"<html><body><embed src="http://embed.example.com/plugin.swf"></body></html>"#,
        );
        assert!(
            dom.resource_urls()
                .contains(&"http://embed.example.com/plugin.swf".to_owned())
        );
    }

    #[test]
    fn resource_urls_includes_object_data() {
        let dom = Dom::parse(
            r#"<html><body><object data="http://object.example.com/file.pdf"></object></body></html>"#,
        );
        assert!(
            dom.resource_urls()
                .contains(&"http://object.example.com/file.pdf".to_owned())
        );
    }
}
