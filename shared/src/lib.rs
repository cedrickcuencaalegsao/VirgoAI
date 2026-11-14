use iced::widget::scrollable;

pub fn code_scroll_id() -> scrollable::Id {
    scrollable::Id::new("code-scroll")
}
pub fn terminal_scroll_id() -> scrollable::Id {
    scrollable::Id::new("term-scroll")
}
pub fn chat_scroll_id() -> scrollable::Id {
    scrollable::Id::new("chat-scroll")
}