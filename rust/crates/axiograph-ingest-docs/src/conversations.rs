//! Conversation and transcript parsing
//!
//! Extracts knowledge from:
//! - Chat transcripts (Slack, Teams, etc.)
//! - Meeting transcripts
//! - Interview notes
//! - Q&A sessions

#![allow(unused_imports)]

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{Chunk, DocumentExtraction};

const MAX_CONVERSATION_BYTES: usize = 32 * 1024 * 1024;
const MAX_CONVERSATION_TURNS: usize = 100_000;
const MAX_CONVERSATION_PARTICIPANTS: usize = 10_000;

/// A conversation turn
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub speaker: String,
    pub timestamp: Option<String>,
    pub content: String,
    pub is_question: bool,
}

/// A parsed conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub conversation_id: String,
    pub participants: Vec<String>,
    pub turns: Vec<Turn>,
    pub topic: Option<String>,
    pub source: String,
}

/// Parse a Slack-style transcript
pub fn parse_slack_transcript(text: &str, conv_id: &str) -> Result<Conversation> {
    validate_conversation_input(text, conv_id)?;
    // Pattern: "Speaker (HH:MM): message" or "Speaker: message"
    let re = Regex::new(r"(?m)^([A-Za-z0-9_\s]+?)(?:\s*\(([^\)]+)\))?\s*:\s*(.+)$").unwrap();

    let mut turns = Vec::new();
    let mut participants = std::collections::HashSet::new();

    for caps in re.captures_iter(text) {
        let speaker = caps
            .get(1)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        let timestamp = caps.get(2).map(|m| m.as_str().to_string());
        let content = caps
            .get(3)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();

        if !speaker.is_empty() {
            if turns.len() >= MAX_CONVERSATION_TURNS {
                return Err(anyhow!(
                    "conversation turn count exceeds {MAX_CONVERSATION_TURNS}"
                ));
            }
            participants.insert(speaker.clone());
            if participants.len() > MAX_CONVERSATION_PARTICIPANTS {
                return Err(anyhow!(
                    "conversation participant count exceeds {MAX_CONVERSATION_PARTICIPANTS}"
                ));
            }
            turns.push(Turn {
                speaker: speaker.clone(),
                timestamp,
                is_question: content.ends_with('?'),
                content,
            });
        }
    }

    Ok(Conversation {
        conversation_id: conv_id.to_string(),
        participants: participants.into_iter().collect(),
        turns,
        topic: None,
        source: "slack".to_string(),
    })
}

/// Parse a meeting transcript (speaker-labeled paragraphs)
pub fn parse_meeting_transcript(text: &str, conv_id: &str) -> Result<Conversation> {
    validate_conversation_input(text, conv_id)?;
    // Pattern: "SPEAKER NAME:" followed by content
    let re = Regex::new(r"(?m)^([A-Z][A-Za-z\s]+):\s*").unwrap();

    let mut turns = Vec::new();
    let mut participants = std::collections::HashSet::new();
    let mut current_speaker = String::new();
    let mut current_content = String::new();

    for line in text.lines() {
        if let Some(caps) = re.captures(line) {
            // Save previous turn
            if !current_speaker.is_empty() && !current_content.trim().is_empty() {
                if turns.len() >= MAX_CONVERSATION_TURNS {
                    return Err(anyhow!(
                        "conversation turn count exceeds {MAX_CONVERSATION_TURNS}"
                    ));
                }
                turns.push(Turn {
                    speaker: current_speaker.clone(),
                    timestamp: None,
                    is_question: current_content.trim().ends_with('?'),
                    content: current_content.trim().to_string(),
                });
            }

            current_speaker = caps.get(1).unwrap().as_str().trim().to_string();
            participants.insert(current_speaker.clone());
            if participants.len() > MAX_CONVERSATION_PARTICIPANTS {
                return Err(anyhow!(
                    "conversation participant count exceeds {MAX_CONVERSATION_PARTICIPANTS}"
                ));
            }
            current_content = line[caps.get(0).unwrap().end()..].to_string();
        } else if !current_speaker.is_empty() {
            current_content.push(' ');
            current_content.push_str(line.trim());
        }
    }

    // Save last turn
    if !current_speaker.is_empty() && !current_content.trim().is_empty() {
        if turns.len() >= MAX_CONVERSATION_TURNS {
            return Err(anyhow!(
                "conversation turn count exceeds {MAX_CONVERSATION_TURNS}"
            ));
        }
        turns.push(Turn {
            speaker: current_speaker,
            timestamp: None,
            is_question: current_content.trim().ends_with('?'),
            content: current_content.trim().to_string(),
        });
    }

    Ok(Conversation {
        conversation_id: conv_id.to_string(),
        participants: participants.into_iter().collect(),
        turns,
        topic: None,
        source: "meeting".to_string(),
    })
}

fn validate_conversation_input(text: &str, conv_id: &str) -> Result<()> {
    if text.len() > MAX_CONVERSATION_BYTES {
        return Err(anyhow!(
            "conversation text exceeds {MAX_CONVERSATION_BYTES} bytes"
        ));
    }
    if conv_id.is_empty() || conv_id.len() > 1024 {
        return Err(anyhow!("conversation id must be in 1..=1024 bytes"));
    }
    Ok(())
}

/// Extract knowledge chunks from a conversation
pub fn conversation_to_chunks(conv: &Conversation) -> Vec<Chunk> {
    conv.turns
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.is_question) // Skip questions
        .map(|(i, turn)| {
            let mut metadata = HashMap::new();
            metadata.insert("speaker".to_string(), turn.speaker.clone());
            metadata.insert("source_type".to_string(), "conversation".to_string());
            metadata.insert("conversation_id".to_string(), conv.conversation_id.clone());
            if let Some(ts) = &turn.timestamp {
                metadata.insert("timestamp".to_string(), ts.clone());
            }
            if let Some(topic) = &conv.topic {
                metadata.insert("topic".to_string(), topic.clone());
            }

            Chunk {
                chunk_id: format!("{}_{}", conv.conversation_id, i),
                document_id: conv.conversation_id.clone(),
                page: None,
                span_id: format!("turn_{i}"),
                text: turn.content.clone(),
                bbox: None,
                metadata,
            }
        })
        .collect()
}

/// Convert conversation to document extraction
pub fn conversation_to_extraction(conv: &Conversation) -> DocumentExtraction {
    DocumentExtraction {
        source_path: conv.source.clone(),
        document_id: conv.conversation_id.clone(),
        title: conv.topic.clone(),
        chunks: conversation_to_chunks(conv),
        metadata: {
            let mut m = HashMap::new();
            m.insert("type".to_string(), "conversation".to_string());
            m.insert("participants".to_string(), conv.participants.join(", "));
            m
        },
    }
}

/// Identify technical content in conversation turns
pub fn identify_technical_content(conv: &Conversation) -> Vec<(usize, Vec<String>)> {
    let technical_keywords = [
        "rpm",
        "sfm",
        "feed",
        "speed",
        "depth",
        "cut",
        "tool",
        "insert",
        "carbide",
        "hss",
        "material",
        "aluminum",
        "steel",
        "titanium",
        "chatter",
        "vibration",
        "finish",
        "tolerance",
        "dimension",
        "coolant",
        "chip",
        "wear",
        "hardness",
        "roughing",
        "finishing",
    ];

    conv.turns
        .iter()
        .enumerate()
        .filter_map(|(i, turn)| {
            let lower = turn.content.to_lowercase();
            let matches: Vec<String> = technical_keywords
                .iter()
                .filter(|kw| lower.contains(*kw))
                .map(|s| s.to_string())
                .collect();

            if matches.is_empty() {
                None
            } else {
                Some((i, matches))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_slack() {
        let text = r#"
John (10:30): We should reduce the feed rate for titanium
Jane (10:31): What about the speed?
John (10:32): Keep it around 100 SFM for roughing
"#;
        let conv = parse_slack_transcript(text, "test_conv").expect("parse transcript");
        assert_eq!(conv.turns.len(), 3);
        assert!(conv.participants.contains(&"John".to_string()));
        assert!(conv.turns[1].is_question);
    }
}
