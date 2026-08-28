//! Bounded-allocation tokenization for one REPL command line.

fn split_command_line(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(character) = chars.next() {
        match character {
            '"' => in_quotes = !in_quotes,
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            character if character.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }

    if !current.is_empty() {
        out.push(current);
    }

    out
}

/// Tokenize one REPL line while preserving the exact AxQL query suffix.
///
/// Callers must bound the input frame before invoking this function. Allocation
/// is linear in `line.len()` and no recursion is used.
pub fn tokenize_repl_line(line: &str) -> Vec<String> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }

    // AxQL string literals are semantic input, so preserve quotes in the raw
    // query suffix rather than routing them through the convenience tokenizer.
    let cmd_end = line
        .char_indices()
        .find_map(|(index, character)| character.is_whitespace().then_some(index));
    let (command, rest) = match cmd_end {
        Some(index) => (&line[..index], line[index..].trim_start()),
        None => (line, ""),
    };

    if command != "q" && command != "axql" {
        return split_command_line(line);
    }

    let mut out = vec![command.to_string()];
    if rest.is_empty() {
        return out;
    }

    let rest_tokens = split_command_line(rest);
    let mut prefix_token_count = 0_usize;
    while prefix_token_count < rest_tokens.len() {
        let token = &rest_tokens[prefix_token_count];
        if !token.starts_with('-') {
            break;
        }
        prefix_token_count += 1;
        if matches!(token.as_str(), "--apply-refinement" | "--apply")
            && prefix_token_count < rest_tokens.len()
        {
            prefix_token_count += 1;
        }
    }
    out.extend(rest_tokens.iter().take(prefix_token_count).cloned());

    let mut byte_index = 0_usize;
    let bytes = rest.as_bytes();
    for _ in 0..prefix_token_count {
        while byte_index < bytes.len() && bytes[byte_index].is_ascii_whitespace() {
            byte_index += 1;
        }
        while byte_index < bytes.len() && !bytes[byte_index].is_ascii_whitespace() {
            byte_index += 1;
        }
    }

    let raw_query = rest[byte_index..].trim_start();
    if !raw_query.is_empty() {
        out.push(raw_query.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_option_and_query_boundaries_remain_valid_utf8() {
        let tokens =
            tokenize_repl_line("q --apply-refinement 修復 select ?人物 where name(\"Åsa\")");
        assert_eq!(tokens[0], "q");
        assert_eq!(tokens[1], "--apply-refinement");
        assert_eq!(tokens[2], "修復");
        assert_eq!(tokens[3], "select ?人物 where name(\"Åsa\")");
    }
}
