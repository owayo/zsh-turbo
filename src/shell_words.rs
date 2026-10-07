/// zsh の語を区切る空白。全角空白や NBSP は語の一部として扱う。
pub(crate) fn is_blank(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n')
}
