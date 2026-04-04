pub fn button<'a>(label: &'a str, is_selected: bool) -> String {
    if is_selected {
        format!("[ {} ]", label)
    } else {
        format!("  {}  ", label)
    }
}
