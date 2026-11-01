//! Luau's words, for coloring scripts: a tokenizer that splits a script into the kinds of thing
//! the Script screen colors, as Godot's script editor colors GDScript. It reads a whole script at
//! once, so comments and strings that span lines color as they should, and never fails: what
//! isn't Luau colors as plain text.

use std::ops::Range;

/// What a token is, by how it's colored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Declarations and operators that are words: `local`, `function`, `and`, `self`.
    Keyword,
    /// Words that steer what runs next: `if`, `for`, `return`.
    Control,
    /// `true`, `false` and `nil`.
    Literal,
    Number,
    String,
    Comment,
    /// The engine's tables and built-ins, and Luau's standard library.
    Global,
    /// A name called as a function.
    Function,
    /// Any other name.
    Name,
    /// Operators and punctuation.
    Symbol,
}

/// A run of a script's text, by its bytes, and what it is. Spaces between tokens aren't tokens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub range: Range<usize>,
}

const KEYWORDS: [&str; 7] = ["local", "function", "end", "and", "or", "not", "self"];
const CONTROL: [&str; 13] = [
    "if", "then", "elseif", "else", "for", "in", "while", "do", "repeat", "until", "return",
    "break", "continue",
];
/// Words that are keywords only where a name couldn't be: `type Point = ...`, but `type(x)`.
const CONTEXTUAL: [&str; 3] = ["type", "export", "continue"];
const LITERALS: [&str; 3] = ["true", "false", "nil"];
const GLOBALS: [&str; 39] = [
    // The engine's.
    "graphics",
    "input",
    "audio",
    "tilemap",
    "grid",
    "scene",
    "spawn",
    "wait",
    "wait_until",
    // Luau's.
    "print",
    "require",
    "math",
    "string",
    "table",
    "coroutine",
    "bit32",
    "utf8",
    "buffer",
    "vector",
    "debug",
    "os",
    "type",
    "typeof",
    "tostring",
    "tonumber",
    "pairs",
    "ipairs",
    "next",
    "select",
    "error",
    "assert",
    "pcall",
    "xpcall",
    "setmetatable",
    "getmetatable",
    "rawget",
    "rawset",
    "rawequal",
    "unpack",
];

/// The tokens of `text`, in order.
pub fn tokens(text: &str) -> Vec<Token> {
    let mut lexer = Lexer {
        text,
        bytes: text.as_bytes(),
        at: 0,
        tokens: Vec::new(),
    };
    lexer.code(false);
    lexer.tokens
}

/// The token at byte `at` of a script, or the one just before it, as for a cursor at the end of a
/// word.
pub fn token_at(tokens: &[Token], at: usize) -> Option<&Token> {
    let i = tokens.partition_point(|token| token.range.end < at);
    tokens.get(i).filter(|token| token.range.start <= at)
}

/// The token byte `at` of a script is in, if it's in one.
pub fn token_containing(tokens: &[Token], at: usize) -> Option<&Token> {
    let i = tokens.partition_point(|token| token.range.end <= at);
    tokens.get(i).filter(|token| token.range.start <= at)
}

struct Lexer<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
    tokens: Vec<Token>,
}

impl Lexer<'_> {
    fn peek(&self, ahead: usize) -> u8 {
        self.bytes.get(self.at + ahead).copied().unwrap_or(0)
    }

    fn push(&mut self, kind: Kind, start: usize) {
        if start < self.at {
            self.tokens.push(Token {
                kind,
                range: start..self.at,
            });
        }
    }

    /// Reads code to the end, or with `in_braces`, to the `}` that closes the braces it's in,
    /// as in an interpolated string, leaving the `}` to read.
    fn code(&mut self, in_braces: bool) {
        let mut depth = 0;
        while self.at < self.bytes.len() {
            let start = self.at;
            let byte = self.peek(0);
            match byte {
                b' ' | b'\t' | b'\r' | b'\n' => self.at += 1,
                b'-' if self.peek(1) == b'-' => {
                    self.at += 2;
                    if self.long_bracket().is_none() {
                        self.skip_line();
                    }
                    self.push(Kind::Comment, start);
                }
                b'"' | b'\'' => {
                    self.quoted(byte);
                    self.push(Kind::String, start);
                }
                b'`' => self.interpolated(),
                b'[' if matches!(self.peek(1), b'[' | b'=') && self.long_bracket().is_some() => {
                    self.push(Kind::String, start);
                }
                b'0'..=b'9' => self.number(),
                b'.' if self.peek(1).is_ascii_digit() => self.number(),
                b'{' => {
                    depth += 1;
                    self.at += 1;
                    self.push(Kind::Symbol, start);
                }
                b'}' => {
                    if in_braces && depth == 0 {
                        return;
                    }
                    depth -= 1;
                    self.at += 1;
                    self.push(Kind::Symbol, start);
                }
                _ if byte == b'_' || byte.is_ascii_alphabetic() => self.name(),
                _ => {
                    // A whole character, which may be more than one byte.
                    let width = self.text[self.at..]
                        .chars()
                        .next()
                        .map_or(1, char::len_utf8);
                    self.at += width;
                    self.push(Kind::Symbol, start);
                }
            }
        }
    }

    fn skip_line(&mut self) {
        while self.at < self.bytes.len() && self.peek(0) != b'\n' {
            self.at += 1;
        }
    }

    /// Reads a long bracket, `[[ ... ]]` or `[==[ ... ]==]`, if one starts here, to its end or the
    /// script's.
    fn long_bracket(&mut self) -> Option<()> {
        if self.peek(0) != b'[' {
            return None;
        }
        let mut level = 1;
        while self.peek(level) == b'=' {
            level += 1;
        }
        if self.peek(level) != b'[' {
            return None;
        }
        let close = format!("]{}]", "=".repeat(level - 1));
        let body = self.at + level + 1;
        self.at = match self.text[body..].find(&close) {
            Some(end) => body + end + close.len(),
            None => self.bytes.len(),
        };
        Some(())
    }

    /// Reads a string in `quote`s, to its closing quote or the end of its line.
    fn quoted(&mut self, quote: u8) {
        self.at += 1;
        while self.at < self.bytes.len() {
            match self.peek(0) {
                b'\\' => self.at += 2,
                b'\n' => return,
                byte => {
                    self.at += 1;
                    if byte == quote {
                        return;
                    }
                }
            }
        }
        self.at = self.at.min(self.bytes.len());
    }

    /// Reads an interpolated string, whose `{...}` are code.
    fn interpolated(&mut self) {
        let mut start = self.at;
        self.at += 1;
        while self.at < self.bytes.len() {
            match self.peek(0) {
                b'\\' => self.at = (self.at + 2).min(self.bytes.len()),
                b'`' => {
                    self.at += 1;
                    break;
                }
                b'{' => {
                    self.at += 1;
                    self.push(Kind::String, start);
                    self.code(true);
                    start = self.at;
                    if self.peek(0) == b'}' {
                        self.at += 1;
                    }
                }
                _ => self.at += 1,
            }
        }
        self.push(Kind::String, start);
    }

    fn number(&mut self) {
        let start = self.at;
        let hex = self.peek(0) == b'0' && matches!(self.peek(1), b'x' | b'X' | b'b' | b'B');
        if hex {
            self.at += 2;
        }
        while self.at < self.bytes.len() {
            let byte = self.peek(0);
            let exponent = !hex && matches!(byte, b'e' | b'E');
            if exponent && matches!(self.peek(1), b'+' | b'-') {
                self.at += 2;
            } else if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' {
                self.at += 1;
            } else {
                break;
            }
        }
        self.push(Kind::Number, start);
    }

    fn name(&mut self) {
        let start = self.at;
        while self.peek(0) == b'_' || self.peek(0).is_ascii_alphanumeric() {
            self.at += 1;
        }
        let word = &self.text[start..self.at];
        let after = self.text[self.at..].trim_start().chars().next();
        // A field, as in `player.update`, is never a keyword or global, but `a .. b` isn't one.
        let field = match self.tokens.as_slice() {
            [.., before, last] if &self.text[last.range.clone()] == "." => {
                before.range.end != last.range.start || &self.text[before.range.clone()] != "."
            }
            [.., last] => matches!(&self.text[last.range.clone()], "." | ":"),
            [] => false,
        };
        let called = after == Some('(');
        let kind = if field {
            if called { Kind::Function } else { Kind::Name }
        } else if CONTEXTUAL.contains(&word) {
            let as_name = matches!(after, Some('(' | '=' | '.' | ':' | '[' | ',' | ')'));
            if as_name && GLOBALS.contains(&word) {
                Kind::Global
            } else if as_name {
                Kind::Name
            } else if word == "continue" {
                Kind::Control
            } else {
                Kind::Keyword
            }
        } else if KEYWORDS.contains(&word) {
            Kind::Keyword
        } else if CONTROL.contains(&word) {
            Kind::Control
        } else if LITERALS.contains(&word) {
            Kind::Literal
        } else if GLOBALS.contains(&word) {
            Kind::Global
        } else if called {
            Kind::Function
        } else {
            Kind::Name
        };
        self.push(kind, start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each token of `text` as its text and kind.
    fn kinds(text: &str) -> Vec<(&str, Kind)> {
        tokens(text)
            .into_iter()
            .map(|token| (&text[token.range], token.kind))
            .collect()
    }

    #[test]
    fn words_are_keywords_names_globals_and_calls() {
        use Kind::*;
        assert_eq!(
            kinds("local function update(dt: number)\n    if not paused then return end"),
            [
                ("local", Keyword),
                ("function", Keyword),
                ("update", Function),
                ("(", Symbol),
                ("dt", Name),
                (":", Symbol),
                ("number", Name),
                (")", Symbol),
                ("if", Control),
                ("not", Keyword),
                ("paused", Name),
                ("then", Control),
                ("return", Control),
                ("end", Keyword),
            ]
        );
        assert_eq!(
            kinds("graphics.draw_sprite(hero.graphics, x)"),
            [
                ("graphics", Global),
                (".", Symbol),
                ("draw_sprite", Function),
                ("(", Symbol),
                ("hero", Name),
                (".", Symbol),
                ("graphics", Name),
                (",", Symbol),
                ("x", Name),
                (")", Symbol),
            ]
        );
    }

    #[test]
    fn contextual_keywords_are_names_where_names_go() {
        use Kind::*;
        assert_eq!(
            kinds("export type Point = { x: number }")[..2],
            [("export", Keyword), ("type", Keyword)]
        );
        assert_eq!(kinds("type(x)")[0], ("type", Global));
        // After a comment that ends as a field would start, and after joining strings.
        assert_eq!(kinds("-- all the way.\nlocal x")[1], ("local", Keyword));
        assert_eq!(kinds("a .. print")[3], ("print", Global));
        assert_eq!(
            kinds("for _, v in t do continue end")[7],
            ("continue", Control)
        );
        assert_eq!(kinds("local continue = 1")[1], ("continue", Name));
    }

    #[test]
    fn comments_and_strings_run_to_their_ends() {
        use Kind::*;
        assert_eq!(
            kinds("x = 1 -- one\n--[==[ a\n]] still ]==] y"),
            [
                ("x", Name),
                ("=", Symbol),
                ("1", Number),
                ("-- one", Comment),
                ("--[==[ a\n]] still ]==]", Comment),
                ("y", Name),
            ]
        );
        assert_eq!(
            kinds(r#"s = "a \" b" .. 'c' .. [[d]]"#),
            [
                ("s", Name),
                ("=", Symbol),
                (r#""a \" b""#, String),
                (".", Symbol),
                (".", Symbol),
                ("'c'", String),
                (".", Symbol),
                (".", Symbol),
                ("[[d]]", String),
            ]
        );
        // An unclosed string ends with its line.
        assert_eq!(kinds("\"open\nx"), [("\"open", String), ("x", Name)]);
    }

    #[test]
    fn interpolated_strings_hold_code() {
        use Kind::*;
        assert_eq!(
            kinds("`hp {player.hp}/{ {1}[1] }`"),
            [
                ("`hp {", String),
                ("player", Name),
                (".", Symbol),
                ("hp", Name),
                ("}/{", String),
                ("{", Symbol),
                ("1", Number),
                ("}", Symbol),
                ("[", Symbol),
                ("1", Number),
                ("]", Symbol),
                ("}`", String),
            ]
        );
    }

    #[test]
    fn numbers_take_every_form() {
        for number in ["42", "3.25", ".5", "1e-3", "0xff_ff", "0b1010", "1_000"] {
            assert_eq!(kinds(number), [(number, Kind::Number)], "{number}");
        }
        assert_eq!(
            kinds("1..2"),
            [("1..2", Kind::Number)],
            "Luau reads it as a malformed number too"
        );
    }

    #[test]
    fn the_token_at_a_cursor_is_found() {
        let text = "local name = 1";
        let tokens = tokens(text);
        let at = |byte| token_at(&tokens, byte).map(|token| &text[token.range.clone()]);
        assert_eq!(at(7), Some("name"));
        // Just after a word, as a cursor is after typing it.
        assert_eq!(at(10), Some("name"));
        assert_eq!(at(11), Some("="));
        assert_eq!(at(5), Some("local"));
        let containing = |byte| token_containing(&tokens, byte).map(|t| &text[t.range.clone()]);
        assert_eq!(containing(6), Some("name"));
        assert_eq!(containing(10), None);
        assert_eq!(containing(11), Some("="));
    }
}
