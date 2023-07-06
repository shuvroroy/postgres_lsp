/// A super simple lexer for sql statements to work around a weakness of pg_query.rs.
///
/// pg_query.rs only parses valid statements, and no whitespaces or newlines.
/// To circumvent this, we use a very simple lexer that just knows what kind of characters are
/// being used. all words are put into the "Word" type and will be defined in more detail by the results of pg_query.rs
use cstree::text::{TextRange, TextSize};
use logos::Logos;

use crate::{
    parser::Parser, pg_query_utils::get_position_for_pg_query_node, syntax_kind::SyntaxKind,
};

#[derive(Logos, Debug, PartialEq)]
pub enum StatementToken {
    // copied from protobuf::Token. can be generated later
    #[token("%")]
    Ascii37,
    #[token("(")]
    Ascii40,
    #[token(")")]
    Ascii41,
    #[token("*")]
    Ascii42,
    #[token("+")]
    Ascii43,
    #[token(",")]
    Ascii44,
    #[token("-")]
    Ascii45,
    #[token(".")]
    Ascii46,
    #[token("/")]
    Ascii47,
    #[token(":")]
    Ascii58,
    #[token(";")]
    Ascii59,
    #[token("<")]
    Ascii60,
    #[token("=")]
    Ascii61,
    #[token(">")]
    Ascii62,
    #[token("?")]
    Ascii63,
    #[token("[")]
    Ascii91,
    #[token("\\")]
    Ascii92,
    #[token("]")]
    Ascii93,
    #[token("^")]
    Ascii94,
    // comments, whitespaces and keywords
    #[regex("'([^']+)'")]
    Sconst,
    #[regex("(\\w+)"gm)]
    Word,
    #[regex(" +"gm)]
    Whitespace,
    #[regex("\n+"gm)]
    Newline,
    #[regex("\t+"gm)]
    Tab,
    #[regex("/\\*[^*]*\\*+(?:[^/*][^*]*\\*+)*/|--[^\n]*"g)]
    Comment,
}

impl StatementToken {
    /// Creates a `SyntaxKind` from a `StatementToken`.
    /// can be generated.
    pub fn syntax_kind(&self) -> SyntaxKind {
        match self {
            StatementToken::Ascii37 => SyntaxKind::Ascii37,
            StatementToken::Ascii40 => SyntaxKind::Ascii40,
            StatementToken::Ascii41 => SyntaxKind::Ascii41,
            StatementToken::Ascii42 => SyntaxKind::Ascii42,
            StatementToken::Ascii43 => SyntaxKind::Ascii43,
            StatementToken::Ascii44 => SyntaxKind::Ascii44,
            StatementToken::Ascii45 => SyntaxKind::Ascii45,
            StatementToken::Ascii46 => SyntaxKind::Ascii46,
            StatementToken::Ascii47 => SyntaxKind::Ascii47,
            StatementToken::Ascii58 => SyntaxKind::Ascii58,
            StatementToken::Ascii59 => SyntaxKind::Ascii59,
            StatementToken::Ascii60 => SyntaxKind::Ascii60,
            StatementToken::Ascii61 => SyntaxKind::Ascii61,
            StatementToken::Ascii62 => SyntaxKind::Ascii62,
            StatementToken::Ascii63 => SyntaxKind::Ascii63,
            StatementToken::Ascii91 => SyntaxKind::Ascii91,
            StatementToken::Ascii92 => SyntaxKind::Ascii92,
            StatementToken::Ascii93 => SyntaxKind::Ascii93,
            StatementToken::Ascii94 => SyntaxKind::Ascii94,
            StatementToken::Word => SyntaxKind::Word,
            StatementToken::Whitespace => SyntaxKind::Whitespace,
            StatementToken::Newline => SyntaxKind::Newline,
            StatementToken::Tab => SyntaxKind::Tab,
            StatementToken::Sconst => SyntaxKind::Sconst,
            StatementToken::Comment => SyntaxKind::Comment,
            _ => panic!("Unknown StatementToken: {:?}", self),
        }
    }
}

impl Parser {
    pub fn parse_statement(&mut self, text: &str, at_offset: Option<u32>) {
        let offset = at_offset.unwrap_or(0);
        let range = TextRange::new(
            TextSize::from(offset),
            TextSize::from(offset + text.len() as u32),
        );

        let mut pg_query_tokens = match pg_query::scan(text) {
            Ok(scanned) => scanned.tokens.into_iter().peekable(),
            Err(e) => {
                self.error(e.to_string(), range);
                Vec::new().into_iter().peekable()
            }
        };

        let parsed = pg_query::parse(text);
        let proto;
        let mut nodes;
        let mut pg_query_nodes = match parsed {
            Ok(parsed) => {
                proto = parsed.protobuf;

                nodes = proto.nodes();

                nodes.sort_by(|a, b| {
                    get_position_for_pg_query_node(&a.0).cmp(&get_position_for_pg_query_node(&b.0))
                });

                nodes.into_iter().peekable()
            }
            Err(e) => {
                self.error(e.to_string(), range);
                Vec::new().into_iter().peekable()
            }
        };

        let mut lexer = StatementToken::lexer(&text);

        // parse root node if no syntax errors
        if pg_query_nodes.peek().is_some() {
            let (node, depth, _) = pg_query_nodes.next().unwrap();
            self.stmt(node.to_enum(), range);
            self.start_node(SyntaxKind::from_pg_query_node(&node), &depth);
        }

        while let Some(token) = lexer.next() {
            match token {
                Ok(token) => {
                    let span = lexer.span();

                    // consume pg_query nodes until there is none, or the node is outside of the current text span
                    while let Some(node) = pg_query_nodes.peek() {
                        let pos = get_position_for_pg_query_node(&node.0);
                        if span.contains(&usize::try_from(pos).unwrap()) == false {
                            break;
                        } else {
                            // node is within span
                            let (node, depth, _) = pg_query_nodes.next().unwrap();
                            self.start_node(SyntaxKind::from_pg_query_node(&node), &depth);
                        }
                    }

                    // consume pg_query token if it is within the current text span
                    let next_pg_query_token = pg_query_tokens.peek();
                    if next_pg_query_token.is_some()
                        && (span.contains(
                            &usize::try_from(next_pg_query_token.unwrap().start).unwrap(),
                        ) || span
                            .contains(&usize::try_from(next_pg_query_token.unwrap().end).unwrap()))
                    {
                        self.token(
                            SyntaxKind::from_pg_query_token(&pg_query_tokens.next().unwrap()),
                            lexer.slice(),
                        );
                    } else {
                        // fallback to statement token
                        self.token(token.syntax_kind(), lexer.slice());
                    }
                }
                Err(_) => panic!("Unknown SourceFileToken: {:?}", lexer.span()),
            }
        }

        // close up nodes
        self.consume_token_buffer();
        self.close_until_depth(1);
    }
}

#[cfg(test)]
mod tests {
    use std::assert_eq;

    use super::*;

    #[test]
    fn test_statement_lexer() {
        let input = "select * from contact where id = '123 4 5';";

        let mut lex = StatementToken::lexer(&input);

        assert_eq!(lex.next(), Some(Ok(StatementToken::Word)));
        assert_eq!(lex.slice(), "select");

        assert_eq!(lex.next(), Some(Ok(StatementToken::Whitespace)));

        assert_eq!(lex.next(), Some(Ok(StatementToken::Ascii42)));

        assert_eq!(lex.next(), Some(Ok(StatementToken::Whitespace)));

        assert_eq!(lex.next(), Some(Ok(StatementToken::Word)));
        assert_eq!(lex.slice(), "from");

        assert_eq!(lex.next(), Some(Ok(StatementToken::Whitespace)));

        assert_eq!(lex.next(), Some(Ok(StatementToken::Word)));
        assert_eq!(lex.slice(), "contact");

        assert_eq!(lex.next(), Some(Ok(StatementToken::Whitespace)));

        assert_eq!(lex.next(), Some(Ok(StatementToken::Word)));
        assert_eq!(lex.slice(), "where");

        assert_eq!(lex.next(), Some(Ok(StatementToken::Whitespace)));

        assert_eq!(lex.next(), Some(Ok(StatementToken::Word)));
        assert_eq!(lex.slice(), "id");

        assert_eq!(lex.next(), Some(Ok(StatementToken::Whitespace)));

        assert_eq!(lex.next(), Some(Ok(StatementToken::Ascii61)));

        assert_eq!(lex.next(), Some(Ok(StatementToken::Whitespace)));

        assert_eq!(lex.next(), Some(Ok(StatementToken::Sconst)));

        assert_eq!(lex.next(), Some(Ok(StatementToken::Ascii59)));
    }

    #[test]
    fn test_statement_parser() {
        let input = "select *,some_col from contact where id = '123 4 5';";

        let mut parser = Parser::default();
        parser.parse_statement(input, None);
        let parsed = parser.finish();

        assert_eq!(parsed.cst.text(), input);
    }
}