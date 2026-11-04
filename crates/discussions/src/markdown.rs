// SPDX-License-Identifier: AGPL-3.0-or-later
//! Render ordinary Markdown without active HTML, embedded images or executable link schemes.
use pulldown_cmark::{Event, Parser, Tag, TagEnd, html};

fn allowed_link(destination: &str) -> bool {
    !destination
        .chars()
        .any(|character| character.is_control() || character == '\\')
        && (destination.starts_with("https://")
            || destination.starts_with("http://")
            || destination.starts_with('#')
            || (destination.starts_with('/') && !destination.starts_with("//")))
}

pub fn render_markdown(text: &str) -> String {
    let mut link_allowed = true;
    let events = Parser::new(text).filter_map(|event| match event {
        Event::Html(text) | Event::InlineHtml(text) => Some(Event::Text(text)),
        Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => None,
        Event::Start(Tag::Link { ref dest_url, .. }) => {
            link_allowed = allowed_link(dest_url);
            link_allowed.then_some(event)
        }
        Event::End(TagEnd::Link) => link_allowed.then_some(event),
        _ => Some(event),
    });
    let mut output = String::new();
    html::push_html(&mut output, events);
    output
}

#[cfg(test)]
mod tests {
    use super::render_markdown;

    #[test]
    fn rejects_active_content_and_keeps_ordinary_readable_markup() {
        let rendered = render_markdown(
            "*Hi* [local](/files/) [unsafe](javascript&#58;alert(1)) [offsite](//outside.invalid) ![alt](https://outside.invalid/x) <script>bad</script>",
        );
        assert!(rendered.contains("<em>Hi</em>"));
        assert!(rendered.contains("href=\"/files/\""));
        assert!(rendered.contains("unsafe offsite alt"));
        assert!(rendered.contains("&lt;script&gt;"));
        for unsafe_markup in ["javascript:", "href=\"//", "<img", "<script>"] {
            assert!(!rendered.contains(unsafe_markup));
        }
    }
}
