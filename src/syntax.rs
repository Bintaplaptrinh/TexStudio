/// A scheduling heuristic, not a TeX parser. Ignore comments, escaped
/// delimiters, \verb, and verbatim bodies while waiting for balanced input.
pub fn ready(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut braces = 0i32;
    let mut math = false;
    let mut environments: Vec<&str> = Vec::new();
    let mut delimiters: Vec<u8> = Vec::new();
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    return false;
                }
                if bytes[i] == b'(' || bytes[i] == b'[' {
                    delimiters.push(bytes[i]);
                    i += 1;
                    continue;
                }
                if bytes[i] == b')' || bytes[i] == b']' {
                    let open = if bytes[i] == b')' { b'(' } else { b'[' };
                    if delimiters.pop() != Some(open) {
                        return false;
                    }
                    i += 1;
                    continue;
                }
                if !bytes[i].is_ascii_alphabetic() {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let command = &source[start..i];
                if command == "verb" {
                    if bytes.get(i) == Some(&b'*') {
                        i += 1;
                    }
                    let Some(&delimiter) = bytes.get(i) else {
                        return false;
                    };
                    i += 1;
                    while i < bytes.len() && bytes[i] != delimiter {
                        i += 1;
                    }
                    if i == bytes.len() {
                        return false;
                    }
                    i += 1;
                } else if command == "begin" || command == "end" {
                    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                        i += 1;
                    }
                    if bytes.get(i) != Some(&b'{') {
                        return false;
                    }
                    i += 1;
                    let start = i;
                    while i < bytes.len() && bytes[i] != b'}' {
                        i += 1;
                    }
                    if i == bytes.len() {
                        return false;
                    }
                    let env = &source[start..i];
                    i += 1;
                    if command == "begin" {
                        if ["verbatim", "verbatim*", "lstlisting", "minted"].contains(&env) {
                            let end = format!("\\end{{{env}}}");
                            let Some(offset) = source[i..].find(&end) else {
                                return false;
                            };
                            i += offset + end.len();
                        } else {
                            environments.push(env);
                        }
                    } else if environments.pop() != Some(env) {
                        return false;
                    }
                }
            }
            b'{' => {
                braces += 1;
                i += 1;
            }
            b'}' => {
                braces -= 1;
                if braces < 0 {
                    return false;
                }
                i += 1;
            }
            b'$' => {
                math = !math;
                i += 1;
            }
            _ => i += 1,
        }
    }
    braces == 0 && !math && environments.is_empty() && delimiters.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn waits_for_unfinished_tex() {
        for text in [
            r"\frac{a}{",
            r"\begin{align}x=1",
            r"x $y",
            "x\\",
            r"\[ x",
            r"\end{a}",
        ] {
            assert!(!ready(text), "{text}");
        }
        for text in [
            r"\frac{a}{b}",
            "hello % { $\nworld",
            r"\$ \{ \}",
            r"\begin{align}x=1\end{align}",
            r"\verb|{$|",
            r"\begin{verbatim}{$\end{verbatim}",
            r"\[x\]",
            "Tiếng Việt",
        ] {
            assert!(ready(text), "{text}");
        }
    }
}
