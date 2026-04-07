/// Macro to check if any of the given substrings are contained in the text
#[macro_export]
macro_rules! any {
    ($text:expr, [$($sub:expr),+]) => {{
        let text = $text;
        false $(|| text.contains($sub))+
    }};
}
