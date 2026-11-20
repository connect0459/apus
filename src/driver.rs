use std::path::Path;

use crate::ast::Decl;
use crate::check::{CheckError, Signatures, check_func};
use crate::codegen::{CodegenError, compile};
use crate::items::{SplitError, split_items};
use crate::lexer::{LexError, lex};
use crate::link::{LinkError, link};
use crate::parser::{ParseError, parse_item};

#[derive(Debug, Clone, PartialEq)]
pub struct BuildError {
    line: Option<u32>,
    message: String,
}

impl BuildError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            line: None,
            message: message.into(),
        }
    }

    fn at(line: u32, message: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            message: message.into(),
        }
    }

    pub fn line(&self) -> Option<u32> {
        self.line
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl From<LexError> for BuildError {
    fn from(e: LexError) -> Self {
        Self::at(e.line(), e.message())
    }
}

impl From<SplitError> for BuildError {
    fn from(e: SplitError) -> Self {
        Self::at(e.line(), e.message())
    }
}

impl From<ParseError> for BuildError {
    fn from(e: ParseError) -> Self {
        Self::at(e.line(), e.message())
    }
}

impl From<CheckError> for BuildError {
    fn from(e: CheckError) -> Self {
        Self::at(e.line(), e.message())
    }
}

impl From<CodegenError> for BuildError {
    fn from(e: CodegenError) -> Self {
        Self::new(e.message())
    }
}

impl From<LinkError> for BuildError {
    fn from(e: LinkError) -> Self {
        Self::new(e.message())
    }
}

pub fn build(source: &str, output: &Path) -> Result<(), BuildError> {
    let tokens = lex(source)?;
    let mut decls = Vec::new();
    for item in split_items(&tokens)? {
        decls.push(parse_item(item.tokens())?);
    }
    let sigs = Signatures::collect(&decls)?;
    let mut funcs = Vec::new();
    for decl in &decls {
        if let Decl::Func(func) = decl {
            funcs.push(check_func(func, &sigs)?);
        }
    }
    if !funcs.iter().any(|f| f.name() == "main") {
        return Err(BuildError::new("no 'main' function"));
    }
    link(&compile(&funcs, &sigs.layouts())?, output)?;
    Ok(())
}
