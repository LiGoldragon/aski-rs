//! ParseDialect trait — walking a dialect's rules against a token stream.

use crate::synth::types::*;
use super::aski_world::AskiWorld;
use super::tokens::TokenReader;
use super::parse_item::ParseItem;

pub trait ParseDialect {
    fn parse_dialect_rules(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<(), String>;
    fn parse_dialect_looping(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<(), String>;
    fn parse_dialect_until_close(&mut self, reader: &mut TokenReader, parent_id: i64, delimiter: Delimiter) -> Result<(), String>;
}

impl ParseDialect for AskiWorld {
    fn parse_dialect_rules(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<(), String> {
        let dialect_name = self.current_dialect_name().to_string();
        let dialect = self.dialects.get(&dialect_name)
            .ok_or_else(|| format!("unknown dialect: {}", dialect_name))?
            .clone();

        for rule in &dialect.rules {
            match rule {
                Rule::Sequential(items) => {
                    for item in items {
                        self.parse_item(reader, parent_id, item)?;
                    }
                }
                Rule::OrderedChoice(alternatives) => {
                    let mut matched = false;
                    for alt in alternatives {
                        let snap = self.snapshot();
                        let saved = reader.pos;
                        match self.try_parse_items(reader, parent_id, &alt.items) {
                            Ok(()) => { matched = true; break; }
                            Err(_) => {
                                self.restore(&snap);
                                reader.pos = saved;
                            }
                        }
                    }
                    if !matched {
                        return Err(format!("no alternative matched at pos {} in {}", reader.pos, dialect_name));
                    }
                }
            }
        }
        Ok(())
    }

    fn parse_dialect_looping(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<(), String> {
        let dialect_name = self.current_dialect_name().to_string();
        let dialect = self.dialects.get(&dialect_name)
            .ok_or_else(|| format!("unknown dialect: {}", dialect_name))?
            .clone();

        // Collect all rules — Sequential run once first, then OrderedChoice loop
        for rule in &dialect.rules {
            if let Rule::Sequential(items) = rule {
                for item in items {
                    self.parse_item(reader, parent_id, item)?;
                }
            }
        }

        // Collect OrderedChoice rules for looping
        let choices: Vec<&Vec<ChoiceAlternative>> = dialect.rules.iter()
            .filter_map(|r| match r {
                Rule::OrderedChoice(alts) => Some(alts),
                _ => None,
            })
            .collect();

        if choices.is_empty() { return Ok(()); }

        // Track match count per alternative (for cardinality enforcement)
        let total_alts: usize = choices.iter().map(|c| c.len()).sum();
        let mut match_counts: Vec<usize> = vec![0; total_alts];

        // Loop: try each ordered choice, respecting per-alternative cardinality
        loop {
            reader.skip_newlines();
            if reader.at_end() { break; }

            let mut matched = false;
            let mut alt_offset = 0;
            for alts in &choices {
                for (i, alt) in alts.iter().enumerate() {
                    let idx = alt_offset + i;
                    // Check cardinality limit
                    let at_limit = match alt.cardinality {
                        Card::Optional | Card::One => match_counts[idx] >= 1,
                        Card::ZeroOrMore | Card::OneOrMore => false,
                    };
                    if at_limit { continue; }

                    let snap = self.snapshot();
                    let saved = reader.pos;
                    match self.try_parse_items(reader, parent_id, &alt.items) {
                        Ok(()) => {
                            if reader.pos > saved {
                                match_counts[idx] += 1;
                                matched = true;
                                break;
                            }
                        }
                        Err(_) => {
                            self.restore(&snap);
                            reader.pos = saved;
                        }
                    }
                }
                if matched { break; }
                alt_offset += alts.len();
            }
            if !matched { break; }
        }

        // Check minimum cardinality (One and OneOrMore must have matched)
        let mut alt_offset = 0;
        for alts in &choices {
            for (i, alt) in alts.iter().enumerate() {
                let idx = alt_offset + i;
                let required = matches!(alt.cardinality, Card::One | Card::OneOrMore);
                if required && match_counts[idx] == 0 {
                    return Err(format!("required alternative not matched in {}", dialect_name));
                }
            }
            alt_offset += alts.len();
        }

        Ok(())
    }

    fn parse_dialect_until_close(&mut self, reader: &mut TokenReader, parent_id: i64, delimiter: Delimiter) -> Result<(), String> {
        loop {
            reader.skip_newlines();
            if reader.at_end() { break; }
            if reader.is_close(delimiter) { break; }

            let dialect_name = self.current_dialect_name().to_string();
            let dialect = self.dialects.get(&dialect_name)
                .ok_or_else(|| format!("unknown dialect: {}", dialect_name))?
                .clone();

            let snap = self.snapshot();
            let saved = reader.pos;
            match self.parse_dialect_once(reader, parent_id, &dialect) {
                Ok(()) => {
                    if reader.pos <= saved { reader.pos = saved + 1; }
                }
                Err(_) => {
                    self.restore(&snap);
                    reader.pos = saved;
                    break;
                }
            }
        }
        Ok(())
    }
}

impl AskiWorld {
    fn try_parse_items(&mut self, reader: &mut TokenReader, parent_id: i64, items: &[Item]) -> Result<(), String> {
        for item in items {
            self.parse_item(reader, parent_id, item)?;
        }
        Ok(())
    }

    fn parse_dialect_once(&mut self, reader: &mut TokenReader, parent_id: i64, dialect: &Dialect) -> Result<(), String> {
        for rule in &dialect.rules {
            match rule {
                Rule::Sequential(items) => {
                    for item in items {
                        self.parse_item(reader, parent_id, item)?;
                    }
                }
                Rule::OrderedChoice(alternatives) => {
                    let mut matched = false;
                    for alt in alternatives {
                        let snap = self.snapshot();
                        let saved = reader.pos;
                        match self.try_parse_items(reader, parent_id, &alt.items) {
                            Ok(()) => { matched = true; break; }
                            Err(_) => {
                                self.restore(&snap);
                                reader.pos = saved;
                            }
                        }
                    }
                    if !matched {
                        return Err("no alternative matched".into());
                    }
                }
            }
        }
        Ok(())
    }
}
