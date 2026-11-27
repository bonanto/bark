// A tiny Markdown subset for the EULA: #/##/### headings, paragraphs, **bold**, [text](url) and <url>.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    H1,
    H2,
    H3,
    P,
}

pub struct Block {
    pub kind: Kind,
    pub text: String,
}

fn clean(s: &str) -> String {
    // [text](url) -> text ; <url> -> url
    let mut out = String::new();
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < c.len() {
        if c[i] == '[' {
            if let Some(close) = c[i..].iter().position(|&x| x == ']') {
                let j = i + close;
                if j + 1 < c.len() && c[j + 1] == '(' {
                    if let Some(p) = c[j..].iter().position(|&x| x == ')') {
                        out.extend(&c[i + 1..j]);
                        i = j + p + 1;
                        continue;
                    }
                }
            }
        }
        if c[i] == '<' {
            if let Some(close) = c[i..].iter().position(|&x| x == '>') {
                let inner: String = c[i + 1..i + close].iter().collect();
                if inner.starts_with("http") {
                    out.push_str(&inner);
                    i += close + 1;
                    continue;
                }
            }
        }
        if c[i] == '\\' && i + 1 < c.len() {
            out.push(c[i + 1]);
            i += 2;
            continue;
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

pub fn parse(src: &str) -> Vec<Block> {
    let mut v = Vec::new();
    let mut para = String::new();
    let flush = |para: &mut String, v: &mut Vec<Block>| {
        if !para.trim().is_empty() {
            v.push(Block { kind: Kind::P, text: clean(para.trim()) });
        }
        para.clear();
    };
    for line in src.lines() {
        let t = line.trim();
        if t.is_empty() {
            flush(&mut para, &mut v);
        } else if let Some(h) = t.strip_prefix("### ") {
            flush(&mut para, &mut v);
            v.push(Block { kind: Kind::H3, text: clean(h) });
        } else if let Some(h) = t.strip_prefix("## ") {
            flush(&mut para, &mut v);
            v.push(Block { kind: Kind::H2, text: clean(h) });
        } else if let Some(h) = t.strip_prefix("# ") {
            flush(&mut para, &mut v);
            v.push(Block { kind: Kind::H1, text: clean(h) });
        } else {
            if !para.is_empty() {
                para.push(' ');
            }
            para.push_str(t);
        }
    }
    flush(&mut para, &mut v);
    v
}

/// Splits text into (word, bold) pieces; `**` toggles bold.
pub fn words(text: &str, base_bold: bool) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut bold = false;
    for (i, part) in text.split("**").enumerate() {
        if i > 0 {
            bold = !bold;
        }
        for w in part.split_whitespace() {
            out.push((w.to_string(), bold || base_bold));
        }
    }
    out
}
