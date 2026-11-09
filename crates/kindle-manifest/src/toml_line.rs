//! Changing one value in a TOML file people also edit by hand (`kindle.toml`): only that
//! line changes, so comments, order and line endings stay as they were. The game and the
//! launcher both save Kindle's settings this way.

/// `text` with `key = value` set in `[section]` (`value` is already TOML, e.g. `true` or
/// `"F1"`): the key's line is replaced, or added after the section's last setting, or the
/// section is added at the end.
pub fn set_toml_value(text: &str, section: &str, key: &str, value: &str) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let line = format!("{key} = {value}");
    let header = format!("[{section}]");
    let Some(start) = lines.iter().position(|l| l.trim() == header) else {
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push(header);
        lines.push(line);
        return lines.join(newline) + newline;
    };
    let end = lines[start + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |i| start + 1 + i);
    let is_key = |l: &str| l.trim_start().strip_prefix(key).is_some_and(|rest| rest.trim_start().starts_with('='));
    if let Some(i) = (start + 1..end).find(|&i| is_key(&lines[i])) {
        let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
        lines[i] = format!("{indent}{line}");
    } else {
        let last = (start..end).rev().find(|&i| !lines[i].trim().is_empty()).unwrap_or(start);
        lines.insert(last + 1, line);
    }
    lines.join(newline) + newline
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "[runtime]\nhot_reload = true\n\n[overlay]\n# Draw the overlay.\nenabled = true\n\n# The key.\ntoggle_key = \"F1\"\n";

    #[test]
    fn replaces_only_the_line() {
        let out = set_toml_value(CONFIG, "overlay", "enabled", "false");
        assert_eq!(out, CONFIG.replace("enabled = true", "enabled = false"));
    }

    #[test]
    fn adds_a_missing_key_to_its_section() {
        let out = set_toml_value(CONFIG, "overlay", "developer", "true");
        assert!(out.ends_with("toggle_key = \"F1\"\ndeveloper = true\n"));
        let out = set_toml_value(CONFIG, "runtime", "console", "false");
        assert!(out.starts_with("[runtime]\nhot_reload = true\nconsole = false\n\n[overlay]"));
    }

    #[test]
    fn adds_a_missing_section_and_keeps_crlf() {
        let out = set_toml_value(&CONFIG.replace('\n', "\r\n"), "launcher", "theme", "\"dark\"");
        assert!(out.ends_with("toggle_key = \"F1\"\r\n\r\n[launcher]\r\ntheme = \"dark\"\r\n"));
        assert!(!out.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn similar_keys_are_left_alone() {
        let text = "[overlay]\nenabled_extra = 1\nenabled = true\n";
        assert_eq!(set_toml_value(text, "overlay", "enabled", "false"), "[overlay]\nenabled_extra = 1\nenabled = false\n");
    }
}
