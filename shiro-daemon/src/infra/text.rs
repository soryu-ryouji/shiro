// 文本处理：文稿预览提取（markdown 标记剥离）。

use std::path::Path;

/// 读文件头部（最多 max_bytes 字节；截断处的多字节字符经 lossy 变替换符，由剥离逻辑过滤）
pub(crate) fn read_file_head(path: &Path, max_bytes: usize) -> Option<String> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    f.take(max_bytes as u64).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// 剥离单行 markdown 块级标记（# 标题、> 引用、-/*/+ 与 "1." 列表），返回行内文本
fn strip_markdown_line(line: &str) -> String {
    let mut s = line.trim();
    loop {
        let t = s.trim_start();
        let Some(c) = t.chars().next() else { break };
        match c {
            '#' | '>' | '-' | '*' | '+' => s = &t[c.len_utf8()..],
            '0'..='9' => {
                let digits = t
                    .char_indices()
                    .take_while(|(_, ch)| ch.is_ascii_digit())
                    .count();
                let rest = &t[digits..];
                match rest.strip_prefix(". ") {
                    Some(after) => s = after,
                    None => break,
                }
            }
            _ => break,
        }
    }
    s.trim().to_string()
}

/// 剥离行内 markdown：图片 ![alt](url) 整体移除，链接 [text](url) 保留 text，粗体/行内码/删除线标记移除
fn strip_inline_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    loop {
        let img = rest.find("![");
        let link = rest.find('[');
        // 图片与链接标记取更靠前的一个
        let take_img = match (img, link) {
            (Some(i), Some(l)) => i < l,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if take_img {
            let i = img.expect("take_img 为 true 时 img 必为 Some");
            out.push_str(&rest[..i]);
            match rest[i..]
                .find("](")
                .and_then(|j| rest[i + j..].find(')').map(|k| (j, k)))
            {
                Some((j, k)) => rest = &rest[i + j + k + 1..],
                None => {
                    rest = &rest[i + 2..];
                }
            }
        } else if let Some(i) = link {
            out.push_str(&rest[..i]);
            let Some(j) = rest[i..].find(']') else {
                rest = &rest[i + 1..];
                continue;
            };
            let text = &rest[i + 1..i + j];
            out.push_str(text);
            if let Some(after) = rest[i + j..].strip_prefix("](") {
                match after.find(')') {
                    Some(k) => rest = &after[k + 1..],
                    None => rest = after,
                }
            } else {
                rest = &rest[i + j + 1..];
            }
        } else {
            out.push_str(rest);
            break;
        }
    }
    out.chars()
        .filter(|c| !matches!(c, '*' | '`' | '~'))
        .collect()
}

/// 从文稿内容提取正文预览：markdown 逐行剥离标记，纯文本直接取开头约 max_chars 字
pub(crate) fn excerpt_from_content(content: &str, max_chars: usize, is_markdown: bool) -> String {
    let mut out = String::new();
    for line in content.lines() {
        let text = if is_markdown {
            strip_inline_markdown(&strip_markdown_line(line))
        } else {
            line.trim().to_string()
        };
        if text.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&text);
        if out.chars().count() >= max_chars {
            break;
        }
    }
    if out.chars().count() > max_chars {
        out.chars().take(max_chars).collect()
    } else {
        out
    }
}
