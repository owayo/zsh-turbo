/// zsh の語を区切る空白。全角空白や NBSP は語の一部として扱う。
pub(crate) fn is_blank(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n')
}

/// 先頭が数値 glob (`<N-M>`、N と M は省略可) なら、その文字数を返す。
pub(crate) fn numeric_glob_len(mut chars: impl Iterator<Item = char>) -> Option<usize> {
    if chars.next()? != '<' {
        return None;
    }
    let mut dash = false;
    for (index, c) in chars.enumerate() {
        match c {
            '0'..='9' => {}
            '-' if !dash => dash = true,
            '>' if dash => return Some(index + 2),
            _ => return None,
        }
    }
    None
}
