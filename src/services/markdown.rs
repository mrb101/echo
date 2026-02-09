use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone)]
pub enum MessageBlock {
    RichText(Vec<InlineSpan>),
    CodeBlock {
        language: Option<String>,
        code: String,
    },
    Heading {
        level: u8,
        spans: Vec<InlineSpan>,
    },
    BlockQuote(Vec<MessageBlock>),
    OrderedList(Vec<Vec<MessageBlock>>),
    UnorderedList(Vec<Vec<MessageBlock>>),
    HorizontalRule,
}

#[derive(Debug, Clone)]
pub struct InlineSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub link_url: Option<String>,
}

impl InlineSpan {
    fn new(text: String) -> Self {
        Self {
            text,
            bold: false,
            italic: false,
            strikethrough: false,
            code: false,
            link_url: None,
        }
    }
}

pub fn parse_markdown(input: &str) -> Vec<MessageBlock> {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(input, options);
    let events: Vec<Event> = parser.collect();

    let mut ctx = ParseContext::new();
    ctx.process_events(&events);
    ctx.finish()
}

struct ParseContext {
    blocks: Vec<MessageBlock>,
    // Current inline spans being accumulated
    current_spans: Vec<InlineSpan>,
    // Formatting state stack
    bold: bool,
    italic: bool,
    strikethrough: bool,
    code_inline: bool,
    link_url: Option<String>,
    // Block-level state
    in_code_block: bool,
    code_block_lang: Option<String>,
    code_block_content: String,
    heading_level: Option<u8>,
    heading_spans: Vec<InlineSpan>,
    // Nested structures
    blockquote_depth: u32,
    blockquote_blocks: Vec<Vec<MessageBlock>>,
    list_stack: Vec<ListState>,
}

struct ListState {
    ordered: bool,
    items: Vec<Vec<MessageBlock>>,
    current_item_blocks: Vec<MessageBlock>,
}

impl ParseContext {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            current_spans: Vec::new(),
            bold: false,
            italic: false,
            strikethrough: false,
            code_inline: false,
            link_url: None,
            in_code_block: false,
            code_block_lang: None,
            code_block_content: String::new(),
            heading_level: None,
            heading_spans: Vec::new(),
            blockquote_depth: 0,
            blockquote_blocks: Vec::new(),
            list_stack: Vec::new(),
        }
    }

    fn process_events(&mut self, events: &[Event]) {
        for event in events {
            self.handle_event(event);
        }
    }

    fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Start(tag) => self.handle_start(tag),
            Event::End(tag) => self.handle_end(tag),
            Event::Text(text) => self.handle_text(text),
            Event::Code(code) => self.handle_inline_code(code),
            Event::SoftBreak => self.handle_soft_break(),
            Event::HardBreak => self.handle_hard_break(),
            Event::Rule => self.handle_rule(),
            _ => {}
        }
    }

    fn handle_start(&mut self, tag: &Tag) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.heading_level = Some(heading_level_to_u8(level));
                self.heading_spans.clear();
            }
            Tag::Strong => self.bold = true,
            Tag::Emphasis => self.italic = true,
            Tag::Strikethrough => self.strikethrough = true,
            Tag::Link { dest_url, .. } => {
                self.link_url = Some(dest_url.to_string());
            }
            Tag::CodeBlock(kind) => {
                self.flush_paragraph();
                self.in_code_block = true;
                self.code_block_content.clear();
                self.code_block_lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                        let lang = lang.trim().to_string();
                        if lang.is_empty() {
                            None
                        } else {
                            Some(lang)
                        }
                    }
                    pulldown_cmark::CodeBlockKind::Indented => None,
                };
            }
            Tag::BlockQuote(_) => {
                self.flush_paragraph();
                self.blockquote_depth += 1;
                self.blockquote_blocks.push(Vec::new());
            }
            Tag::List(start) => {
                self.flush_paragraph();
                self.list_stack.push(ListState {
                    ordered: start.is_some(),
                    items: Vec::new(),
                    current_item_blocks: Vec::new(),
                });
            }
            Tag::Item => {
                // Start collecting blocks for this list item
            }
            _ => {}
        }
    }

    fn handle_end(&mut self, tag: &TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                if self.heading_level.is_some() {
                    // Don't flush paragraph inside heading
                    return;
                }
                self.flush_paragraph();
            }
            TagEnd::Heading(_level) => {
                // Flush any remaining spans into heading_spans
                if !self.current_spans.is_empty() {
                    self.heading_spans.append(&mut self.current_spans);
                }
                if let Some(level) = self.heading_level.take() {
                    let spans = std::mem::take(&mut self.heading_spans);
                    self.push_block(MessageBlock::Heading { level, spans });
                }
            }
            TagEnd::Strong => self.bold = false,
            TagEnd::Emphasis => self.italic = false,
            TagEnd::Strikethrough => self.strikethrough = false,
            TagEnd::Link => {
                self.link_url = None;
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                let code = std::mem::take(&mut self.code_block_content);
                let language = self.code_block_lang.take();
                // Trim trailing newline
                let code = code.trim_end_matches('\n').to_string();
                self.push_block(MessageBlock::CodeBlock { language, code });
            }
            TagEnd::BlockQuote(_) => {
                self.flush_paragraph();
                self.blockquote_depth -= 1;
                if let Some(inner_blocks) = self.blockquote_blocks.pop() {
                    self.push_block(MessageBlock::BlockQuote(inner_blocks));
                }
            }
            TagEnd::List(_) => {
                self.flush_paragraph();
                if let Some(mut list_state) = self.list_stack.pop() {
                    // Push any remaining item blocks
                    if !list_state.current_item_blocks.is_empty() {
                        let item_blocks = std::mem::take(&mut list_state.current_item_blocks);
                        list_state.items.push(item_blocks);
                    }
                    let block = if list_state.ordered {
                        MessageBlock::OrderedList(list_state.items)
                    } else {
                        MessageBlock::UnorderedList(list_state.items)
                    };
                    self.push_block(block);
                }
            }
            TagEnd::Item => {
                self.flush_paragraph_into_list();
                if let Some(list_state) = self.list_stack.last_mut() {
                    let item_blocks = std::mem::take(&mut list_state.current_item_blocks);
                    list_state.items.push(item_blocks);
                }
            }
            _ => {}
        }
    }

    fn handle_text(&mut self, text: &pulldown_cmark::CowStr) {
        if self.in_code_block {
            self.code_block_content.push_str(text);
            return;
        }

        let span = InlineSpan {
            text: text.to_string(),
            bold: self.bold,
            italic: self.italic,
            strikethrough: self.strikethrough,
            code: self.code_inline,
            link_url: self.link_url.clone(),
        };

        if self.heading_level.is_some() {
            self.heading_spans.push(span);
        } else {
            self.current_spans.push(span);
        }
    }

    fn handle_inline_code(&mut self, code: &pulldown_cmark::CowStr) {
        let span = InlineSpan {
            text: code.to_string(),
            bold: self.bold,
            italic: self.italic,
            strikethrough: self.strikethrough,
            code: true,
            link_url: self.link_url.clone(),
        };

        if self.heading_level.is_some() {
            self.heading_spans.push(span);
        } else {
            self.current_spans.push(span);
        }
    }

    fn handle_soft_break(&mut self) {
        let span = InlineSpan::new(" ".to_string());
        if self.heading_level.is_some() {
            self.heading_spans.push(span);
        } else {
            self.current_spans.push(span);
        }
    }

    fn handle_hard_break(&mut self) {
        let span = InlineSpan::new("\n".to_string());
        if self.heading_level.is_some() {
            self.heading_spans.push(span);
        } else {
            self.current_spans.push(span);
        }
    }

    fn handle_rule(&mut self) {
        self.flush_paragraph();
        self.push_block(MessageBlock::HorizontalRule);
    }

    fn flush_paragraph(&mut self) {
        if self.current_spans.is_empty() {
            return;
        }
        let spans = std::mem::take(&mut self.current_spans);
        self.push_block(MessageBlock::RichText(spans));
    }

    fn flush_paragraph_into_list(&mut self) {
        if self.current_spans.is_empty() {
            return;
        }
        let spans = std::mem::take(&mut self.current_spans);
        let block = MessageBlock::RichText(spans);
        if let Some(list_state) = self.list_stack.last_mut() {
            list_state.current_item_blocks.push(block);
        }
    }

    fn push_block(&mut self, block: MessageBlock) {
        if !self.list_stack.is_empty() {
            if let Some(list_state) = self.list_stack.last_mut() {
                list_state.current_item_blocks.push(block);
                return;
            }
        }
        if self.blockquote_depth > 0 {
            if let Some(bq_blocks) = self.blockquote_blocks.last_mut() {
                bq_blocks.push(block);
                return;
            }
        }
        self.blocks.push(block);
    }

    fn finish(mut self) -> Vec<MessageBlock> {
        self.flush_paragraph();
        self.blocks
    }
}

fn heading_level_to_u8(level: &HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Convert inline spans to Pango markup string
pub fn spans_to_pango_markup(spans: &[InlineSpan]) -> String {
    let mut markup = String::new();
    for span in spans {
        // Open tags
        if let Some(url) = &span.link_url {
            markup.push_str("<a href=\"");
            markup.push_str(&glib::markup_escape_text(url));
            markup.push_str("\">");
        }
        if span.strikethrough {
            markup.push_str("<s>");
        }
        if span.italic {
            markup.push_str("<i>");
        }
        if span.bold {
            markup.push_str("<b>");
        }
        if span.code {
            markup.push_str("<tt>");
        }

        // Text content
        markup.push_str(&glib::markup_escape_text(&span.text));

        // Close tags (reverse order)
        if span.code {
            markup.push_str("</tt>");
        }
        if span.bold {
            markup.push_str("</b>");
        }
        if span.italic {
            markup.push_str("</i>");
        }
        if span.strikethrough {
            markup.push_str("</s>");
        }
        if span.link_url.is_some() {
            markup.push_str("</a>");
        }
    }
    markup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let blocks = parse_markdown("Hello world");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            MessageBlock::RichText(spans) => {
                assert_eq!(spans.len(), 1);
                assert_eq!(spans[0].text, "Hello world");
                assert!(!spans[0].bold);
            }
            _ => panic!("Expected RichText"),
        }
    }

    #[test]
    fn test_bold_italic() {
        let blocks = parse_markdown("**bold** and *italic*");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            MessageBlock::RichText(spans) => {
                assert!(spans.iter().any(|s| s.bold && s.text == "bold"));
                assert!(spans.iter().any(|s| s.italic && s.text == "italic"));
            }
            _ => panic!("Expected RichText"),
        }
    }

    #[test]
    fn test_code_block() {
        let blocks = parse_markdown("```rust\nfn main() {}\n```");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            MessageBlock::CodeBlock { language, code } => {
                assert_eq!(language.as_deref(), Some("rust"));
                assert_eq!(code, "fn main() {}");
            }
            _ => panic!("Expected CodeBlock"),
        }
    }

    #[test]
    fn test_heading() {
        let blocks = parse_markdown("# Hello");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            MessageBlock::Heading { level, spans } => {
                assert_eq!(*level, 1);
                assert_eq!(spans[0].text, "Hello");
            }
            _ => panic!("Expected Heading"),
        }
    }

    #[test]
    fn test_unordered_list() {
        let blocks = parse_markdown("- one\n- two\n- three");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            MessageBlock::UnorderedList(items) => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected UnorderedList"),
        }
    }

    #[test]
    fn test_blockquote() {
        let blocks = parse_markdown("> quoted text");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            MessageBlock::BlockQuote(inner) => {
                assert!(!inner.is_empty());
            }
            _ => panic!("Expected BlockQuote"),
        }
    }

    #[test]
    fn test_horizontal_rule() {
        let blocks = parse_markdown("above\n\n---\n\nbelow");
        assert!(blocks.iter().any(|b| matches!(b, MessageBlock::HorizontalRule)));
    }

    #[test]
    fn test_inline_code() {
        let blocks = parse_markdown("Use `foo()` here");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            MessageBlock::RichText(spans) => {
                assert!(spans.iter().any(|s| s.code && s.text == "foo()"));
            }
            _ => panic!("Expected RichText"),
        }
    }
}
