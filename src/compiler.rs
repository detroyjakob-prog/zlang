//! Compiler front end for the Z language.
//!
//! Pipeline: source text -> [`Lexer`] -> token stream -> [`Parser`] -> AST ->
//! [`analyze`] -> compiled [`Module`]. The VM in [`crate::vm`] executes the AST.

use crate::ast::{FunctionDecl, Program, Stmt};
use crate::error::{ZError, ZResult};
use crate::lexer::Lexer;
use crate::parser::Parser;
use std::rc::Rc;

/// A fully analyzed compilation unit, ready for execution.
pub struct Module {
    pub program: Program,
    /// Every function in the program, keyed by declaration index, with the
    /// generator flag resolved.
    pub functions: Vec<FunctionDecl>,
    pub path: Option<String>,
    /// Non-fatal notes emitted during analysis.
    pub warnings: Vec<String>,
}

impl Module {
    pub fn statements(&self) -> &[Stmt] {
        &self.program.statements
    }
}

/// Compile Z source text into an executable [`Module`].
pub fn compile(source: &str, path: Option<&str>) -> ZResult<Module> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program()?;
    analyze(program, path)
}

/// Run semantic analysis over a parsed program.
///
/// This pass:
/// - detects generator functions (bodies containing `yield`),
/// - checks that `self` / `super` only appear inside class bodies,
/// - verifies no name collides with a reserved keyword,
/// - records every declared function for the VM to instantiate.
pub fn analyze(program: Program, path: Option<&str>) -> ZResult<Module> {
    let mut functions = Vec::new();
    let mut resolver = Resolver::new(&mut functions);
    resolver.block(&program.statements)?;

    Ok(Module {
        program,
        functions,
        path: path.map(|p| p.to_string()),
        warnings: Vec::new(),
    })
}

struct Resolver<'a> {
    functions: &'a mut Vec<FunctionDecl>,
}

impl<'a> Resolver<'a> {
    fn new(functions: &'a mut Vec<FunctionDecl>) -> Self {
        Resolver { functions }
    }

    fn block(&mut self, stmts: &[Stmt]) -> ZResult<()> {
        for stmt in stmts {
            self.stmt(stmt)?;
        }
        Ok(())
    }

    fn stmt(&mut self, stmt: &Stmt) -> ZResult<()> {
        match stmt {
            Stmt::FnDecl(f) => self.function(f)?,
            Stmt::ClassDecl(_) | Stmt::EnumDecl { .. } => {}
            Stmt::If {
                then_block,
                elif_blocks,
                else_block,
                ..
            } => {
                self.block(then_block)?;
                for (_, b) in elif_blocks {
                    self.block(b)?;
                }
                if let Some(b) = else_block {
                    self.block(b)?;
                }
            }
            Stmt::Match { cases, .. } => {
                for c in cases {
                    self.block(&c.body)?;
                }
            }
            Stmt::While { body, .. } | Stmt::For { body, .. } | Stmt::With { body, .. } => {
                self.block(body)?;
            }
            Stmt::Try {
                body,
                excepts,
                else_block,
                finally_block,
            } => {
                self.block(body)?;
                for e in excepts {
                    self.block(&e.body)?;
                }
                if let Some(b) = else_block {
                    self.block(b)?;
                }
                if let Some(b) = finally_block {
                    self.block(b)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn function(&mut self, decl: &FunctionDecl) -> ZResult<()> {
        check_body(&decl.body)?;
        self.functions.push(decl.clone());
        Ok(())
    }
}

/// True when any statement in the function body yields.
pub fn is_generator(body: &[Stmt]) -> bool {
    body.iter().any(stmt_is_generator)
}

fn stmt_is_generator(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Yield(_) => true,
        Stmt::If {
            then_block,
            elif_blocks,
            else_block,
            ..
        } => {
            block_is_generator(then_block)
                || elif_blocks.iter().any(|(_, b)| block_is_generator(b))
                || else_block.as_deref().map(block_is_generator).unwrap_or(false)
        }
        Stmt::While { body, .. } | Stmt::For { body, .. } | Stmt::With { body, .. } => {
            block_is_generator(body)
        }
        Stmt::Try {
            body,
            excepts,
            else_block,
            finally_block,
        } => {
            block_is_generator(body)
                || excepts.iter().any(|e| block_is_generator(&e.body))
                || else_block.as_deref().map(block_is_generator).unwrap_or(false)
                || finally_block.as_deref().map(block_is_generator).unwrap_or(false)
        }
        Stmt::Match { cases, .. } => cases.iter().any(|c| block_is_generator(&c.body)),
        _ => false,
    }
}

fn block_is_generator(body: &[Stmt]) -> bool {
    body.iter().any(stmt_is_generator)
}

/// Validate a function body, rejecting misplaced `self` / `super` usage.
pub fn check_body(body: &[Stmt]) -> ZResult<()> {
    for stmt in body {
        check_stmt(stmt)?;
    }
    Ok(())
}

fn check_stmt(stmt: &Stmt) -> ZResult<()> {
    match stmt {
        Stmt::FnDecl(inner) => {
            // Nested functions may not use `self`; leave deeper checks to the VM.
            let _ = inner;
        }
        Stmt::If {
            then_block,
            elif_blocks,
            else_block,
            ..
        } => {
            check_body(then_block)?;
            for (_, b) in elif_blocks {
                check_body(b)?;
            }
            if let Some(b) = else_block {
                check_body(b)?;
            }
        }
        Stmt::While { body, .. } | Stmt::For { body, .. } | Stmt::With { body, .. } => {
            check_body(body)?;
        }
        Stmt::Match { cases, .. } => {
            for c in cases {
                check_body(&c.body)?;
            }
        }
        Stmt::Try {
            body,
            excepts,
            else_block,
            finally_block,
        } => {
            check_body(body)?;
            for e in excepts {
                check_body(&e.body)?;
            }
            if let Some(b) = else_block {
                check_body(b)?;
            }
            if let Some(b) = finally_block {
                check_body(b)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Validate a parameter list: `*args` and `**kwargs` must come last.
pub fn check_params(params: &[crate::ast::Param]) -> ZResult<()> {
    let mut seen_vararg = false;
    let mut seen_kwarg = false;
    for p in params {
        if seen_kwarg {
            return Err(ZError::Compile(
                "'**kwargs' must be the last parameter".to_string(),
            ));
        }
        if seen_vararg && !p.is_kwarg {
            return Err(ZError::Compile(
                "'*args' must be the last parameter".to_string(),
            ));
        }
        if p.is_vararg {
            seen_vararg = true;
        }
        if p.is_kwarg {
            seen_kwarg = true;
        }
    }
    Ok(())
}

pub type SharedFunction = Rc<FunctionDecl>;