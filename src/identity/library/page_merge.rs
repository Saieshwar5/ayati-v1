//! Two changes to the same page as one. A person and an agent often change different
//! sections of a page at the same time; both are kept when they did, and the later
//! save is refused only when both changed the same section.

/// A page's Markdown cut at its headings: each part starts with its heading line (the
/// first part may have none) and is named by that line and how often it came before.
fn parts(text: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = vec![(String::new(), String::new())];
    let mut seen = std::collections::HashMap::new();
    for line in text.lines() {
        if super::page_markdown::heading(line.trim()).is_some() {
            let again = seen.entry(line.trim().to_owned()).or_insert(0);
            *again += 1;
            found.push((format!("{} #{again}", line.trim()), String::new()));
        }
        let part = found.last_mut().expect("one part");
        part.1.push_str(line);
        part.1.push('\n');
    }
    found
}

/// Two changes to the same page, made from `base`, as one page: none when both changed
/// the same part, or either added, removed or reordered parts.
pub fn merge(base: &str, theirs: &str, mine: &str) -> Option<String> {
    let (base, theirs, mine) = (parts(base), parts(theirs), parts(mine));
    let names = |p: &[(String, String)]| p.iter().map(|x| x.0.clone()).collect::<Vec<_>>();
    if names(&base) != names(&theirs) || names(&base) != names(&mine) {
        return None;
    }
    let mut out = String::new();
    for ((b, t), m) in base.iter().zip(&theirs).zip(&mine) {
        let (b, t, m) = (b.1.trim_end(), t.1.trim_end(), m.1.trim_end());
        let kept = if m == b || m == t {
            t
        } else if t == b {
            m
        } else {
            return None;
        };
        if !kept.is_empty() {
            out.push_str(kept);
            out.push_str("\n\n");
        }
    }
    Some(out.trim_end().to_owned() + "\n")
}

#[cfg(test)]
mod tests {
    use super::merge;

    #[test]
    fn changes_to_different_sections_merge_and_to_the_same_section_do_not() {
        let base = "Intro.\n\n# Returns\n\n30 days.\n\n# Shipping\n\nTwo days.\n";
        let theirs = base.replace("Two days", "Three days");
        let mine = base.replace("30 days", "45 days");
        let merged = merge(base, &theirs, &mine).unwrap();
        assert!(
            merged.contains("45 days") && merged.contains("Three days"),
            "{merged}"
        );
        assert_eq!(merge(base, base, &mine).unwrap(), mine);
        assert!(
            merge(base, &base.replace("30 days", "60 days"), &mine).is_none(),
            "the same section"
        );
        assert!(
            merge(base, &format!("{base}\n# New\n\nMore.\n"), &mine).is_none(),
            "a section was added"
        );
    }
}
