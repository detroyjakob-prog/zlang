//! Parser for the Z Programming Language.
//! Recursive descent parser building an AST from a token stream.

use crate::ast::*;
use crate::error::{ZError, ZResult};
use crate::lexer::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    pub fn parse_program(&mut self) -> ZResult<Program> {
        let mut statements = Vec::new();
        self.skip_newlines();
        while !self.is_eof() {
            statements.push(self.parse_statement()?);
            self.end_statement()?;
        }
        Ok(Program { statements })
    }

    fn end_statement(&mut self) -> ZResult<()> {
        match self.cur() {
            TokenKind::Newline => {
                self.advance();
            }
            TokenKind::Eof => {}
            TokenKind::RBrace | TokenKind::RBracket | TokenKind::RParen => {
                return Err(self.err("mismatched closing bracket"));
            }
            _ => {
                return Err(self.err(&format!(
                    "unexpected token {} after statement",
                    tok_desc(&self.cur())
                )));
            }
        }
        self.skip_newlines();
        Ok(())
    }

    fn parse_block(&mut self) -> ZResult<Vec<Stmt>> {
        self.expect(TokenKind::LBrace)?;
        let mut statements = Vec::new();
        self.skip_newlines();
        while !self.check(TokenKind::RBrace) && !self.is_eof() {
            statements.push(self.parse_statement()?);
            self.end_statement()?;
        }
        self.expect(TokenKind::RBrace)?;
        Ok(statements)
    }

    fn parse_statement(&mut self) -> ZResult<Stmt> {
        match &self.cur().clone() {
            TokenKind::Abl => self.parse_var_decl(),
            TokenKind::Const => self.parse_const_decl(),
            TokenKind::Async => self.parse_fn_decl(),
            TokenKind::Static => self.parse_fn_decl(),
            TokenKind::Fn => self.parse_fn_or_lambda_stmt(),
            TokenKind::If => self.parse_if(),
            TokenKind::Match => self.parse_match(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Try => self.parse_try(),
            TokenKind::With => self.parse_with(),
            TokenKind::Import => self.parse_import(),
            TokenKind::From => self.parse_from_import(),
            TokenKind::Dir => self.parse_dir_import(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Yield => self.parse_yield(),
            TokenKind::Callerror => self.parse_callerror(),
            TokenKind::Assert => self.parse_assert(),
            TokenKind::Del => self.parse_del(),
            TokenKind::Global => self.parse_global(),
            TokenKind::Outer => self.parse_outer(),
            TokenKind::Break => {
                self.advance();
                Ok(Stmt::Break)
            }
            TokenKind::Continue => {
                self.advance();
                Ok(Stmt::Continue)
            }
            TokenKind::Pass => {
                self.advance();
                Ok(Stmt::Pass)
            }
            TokenKind::Ident(_) => {
                if self.is_class_decl() {
                    self.parse_class()
                } else {
                    self.parse_assignment_or_expr()
                }
            }
            TokenKind::Enum => self.parse_enum(),
            _ => self.parse_assignment_or_expr(),
        }
    }

    fn parse_fn_or_lambda_stmt(&mut self) -> ZResult<Stmt> {
        if self.check(TokenKind::Fn)
            && matches!(self.peek_at(1), TokenKind::Ident(_))
            && self.peek_at(2) == &TokenKind::LParen
        {
            self.parse_fn_decl()
        } else {
            self.parse_assignment_or_expr()
        }
    }

    fn parse_var_decl(&mut self) -> ZResult<Stmt> {
        self.advance(); // abl
        let mut type_ann = None;

        // Optional type annotation
        if let TokenKind::Ident(id) = &self.cur().clone() {
            if matches!(self.peek_at(1), TokenKind::Assign | TokenKind::Comma) && is_type_name(id)
            {
                type_ann = Some(id.clone());
                self.advance();
            }
        }

        let mut names = vec![self.parse_target_name()?];
        while self.check(TokenKind::Comma) {
            self.advance();
            names.push(self.parse_target_name()?);
        }
        self.expect(TokenKind::Assign)?;
        let mut values = vec![self.parse_expr()?];
        while self.check(TokenKind::Comma) {
            self.advance();
            values.push(self.parse_expr()?);
        }

        if names.len() == 1 {
            Ok(Stmt::VarDecl {
                name: names.into_iter().next().unwrap(),
                type_ann,
                value: values.into_iter().next().unwrap(),
            })
        } else {
            Ok(Stmt::MultiVarDecl { names, values })
        }
    }

    fn parse_target_name(&mut self) -> ZResult<String> {
        match self.cur().clone() {
            TokenKind::Ident(s) => {
                self.advance();
                Ok(s)
            }
            // `self` and `super` are keywords but valid names in parameters.
            TokenKind::Self_ => {
                self.advance();
                Ok("self".to_string())
            }
            TokenKind::Super => {
                self.advance();
                Ok("super".to_string())
            }
            _ => Err(self.err("expected name")),
        }
    }

    fn parse_const_decl(&mut self) -> ZResult<Stmt> {
        self.advance();
        let name = self.parse_target_name()?;
        self.expect(TokenKind::Assign)?;
        let value = self.parse_expr()?;
        Ok(Stmt::ConstDecl { name, value })
    }

    fn parse_assignment_or_expr(&mut self) -> ZResult<Stmt> {
        let expr = self.parse_expr()?;

        if self.is_assign_op() {
            let op = self.parse_assign_op();
            let mut values = vec![self.parse_expr()?];
            while self.check(TokenKind::Comma) {
                self.advance();
                values.push(self.parse_expr()?);
            }
            let target = self.expr_to_target(expr)?;
            if values.len() > 1 {
                let name = match target {
                    Target::Name(n) => n,
                    _ => return Err(self.err("invalid assignment target")),
                };
                return Ok(Stmt::MultiAssign {
                    targets: vec![Target::Name(name)],
                    values,
                });
            }
            return Ok(Stmt::Assign {
                target,
                op,
                value: values.into_iter().next().unwrap(),
            });
        }

        if self.check(TokenKind::Comma) {
            let mut exprs = vec![expr];
            while self.check(TokenKind::Comma) {
                self.advance();
                exprs.push(self.parse_expr()?);
            }
            self.expect(TokenKind::Assign)?;
            let mut values = Vec::new();
            values.push(self.parse_expr()?);
            while self.check(TokenKind::Comma) {
                self.advance();
                values.push(self.parse_expr()?);
            }
            let mut targets = Vec::new();
            for e in exprs {
                targets.push(self.expr_to_target(e)?);
            }
            return Ok(Stmt::MultiAssign { targets, values });
        }

        Ok(Stmt::Expr(expr))
    }

    fn is_assign_op(&self) -> bool {
        matches!(
            self.cur(),
            TokenKind::Assign
                | TokenKind::PlusEq
                | TokenKind::MinusEq
                | TokenKind::StarEq
                | TokenKind::SlashEq
                | TokenKind::FloorDivEq
                | TokenKind::PercentEq
                | TokenKind::PowEq
        )
    }

    fn parse_assign_op(&mut self) -> AssignOp {
        let op = match self.cur() {
            TokenKind::Assign => AssignOp::Assign,
            TokenKind::PlusEq => AssignOp::Add,
            TokenKind::MinusEq => AssignOp::Sub,
            TokenKind::StarEq => AssignOp::Mul,
            TokenKind::SlashEq => AssignOp::Div,
            TokenKind::FloorDivEq => AssignOp::FloorDiv,
            TokenKind::PercentEq => AssignOp::Mod,
            TokenKind::PowEq => AssignOp::Pow,
            _ => return AssignOp::Assign,
        };
        self.advance();
        op
    }

    fn expr_to_target(&self, expr: Expr) -> ZResult<Target> {
        match expr {
            Expr::Ident(s) => Ok(Target::Name(s)),
            Expr::Attribute { obj, attr } => Ok(Target::Attribute(obj, attr)),
            Expr::Index { obj, slice } => match slice {
                Slice::Simple(idx) => Ok(Target::Index(obj, idx)),
                _ => Err(self.err("slice assignment not supported")),
            },
            _ => Err(self.err("invalid assignment target")),
        }
    }

    fn parse_fn_decl(&mut self) -> ZResult<Stmt> {
        let mut is_async = false;
        let mut is_static = false;
        while self.check(TokenKind::Async) || self.check(TokenKind::Static) {
            match self.cur() {
                TokenKind::Async => is_async = true,
                TokenKind::Static => is_static = true,
                _ => {}
            }
            self.advance();
        }
        self.expect(TokenKind::Fn)?;
        let name = self.parse_target_name()?;
        let params = self.parse_params()?;
        let return_type = if self.check(TokenKind::Arrow) {
            self.advance();
            self.parse_type()?
        } else {
            None
        };
        let body = self.parse_block()?;
        Ok(Stmt::FnDecl(FunctionDecl {
            name,
            params,
            body,
            is_async,
            is_static,
            return_type,
        }))
    }

    fn parse_params(&mut self) -> ZResult<Vec<Param>> {
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if self.check(TokenKind::RParen) {
            self.advance();
            return Ok(params);
        }
        loop {
            params.push(self.parse_param()?);
            if self.check(TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(params)
    }

    fn parse_param(&mut self) -> ZResult<Param> {
        if self.check(TokenKind::Star) {
            self.advance();
            let name = self.parse_target_name()?;
            return Ok(Param {
                name,
                type_ann: None,
                default: None,
                is_vararg: true,
                is_kwarg: false,
            });
        }
        if self.check(TokenKind::Pow) && matches!(self.peek_at(1), TokenKind::Ident(_)) {
            self.advance();
            let name = self.parse_target_name()?;
            return Ok(Param {
                name,
                type_ann: None,
                default: None,
                is_vararg: false,
                is_kwarg: true,
            });
        }

        let mut type_ann = None;
        if let TokenKind::Ident(id) = &self.cur().clone() {
            if matches!(self.peek_at(1), TokenKind::Ident(_)) {
                type_ann = Some(id.clone());
                self.advance();
            }
        }
        let name = self.parse_target_name()?;
        let default = if self.check(TokenKind::Assign) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        Ok(Param {
            name,
            type_ann,
            default,
            is_vararg: false,
            is_kwarg: false,
        })
    }

    fn parse_type(&mut self) -> ZResult<Option<String>> {
        match self.cur().clone() {
            TokenKind::Ident(s) => {
                self.advance();
                Ok(Some(s))
            }
            _ => Ok(None),
        }
    }

    fn parse_class(&mut self) -> ZResult<Stmt> {
        let name = self.parse_target_name()?;
        let mut parent = None;
        if self.check(TokenKind::LParen) {
            self.advance();
            if let TokenKind::Ident(p) = &self.cur().clone() {
                parent = Some(p.clone());
                self.advance();
            }
            while self.check(TokenKind::Comma) {
                self.advance();
                if let TokenKind::Ident(_) = &self.cur().clone() {
                    self.advance();
                }
            }
            self.expect(TokenKind::RParen)?;
        }
        self.expect(TokenKind::LBrace)?;
        let mut members = Vec::new();
        self.skip_newlines();
        while !self.check(TokenKind::RBrace) && !self.is_eof() {
            members.push(self.parse_class_member()?);
            self.end_statement()?;
        }
        self.expect(TokenKind::RBrace)?;
        Ok(Stmt::ClassDecl(ClassDecl {
            name,
            parent,
            members,
        }))
    }

    fn parse_class_member(&mut self) -> ZResult<ClassMember> {
        let mut is_static = false;
        while self.check(TokenKind::Static) || self.check(TokenKind::Async) {
            if self.check(TokenKind::Static) {
                is_static = true;
            }
            self.advance();
        }
        if self.check(TokenKind::Fn) {
            self.advance();
            let name = self.parse_target_name()?;
            let params = self.parse_params()?;
            let return_type = if self.check(TokenKind::Arrow) {
                self.advance();
                self.parse_type()?
            } else {
                None
            };
            let body = self.parse_block()?;
            return Ok(ClassMember::Method {
                name,
                params,
                body,
                is_static,
                return_type,
            });
        }
        if self.check(TokenKind::Abl) {
            self.advance();
        }
        let name = self.parse_target_name()?;
        self.expect(TokenKind::Assign)?;
        let value = self.parse_expr()?;
        let const_value = self.const_eval(&value)?;
        Ok(ClassMember::Field { name, value: const_value })
    }

    fn const_eval(&self, expr: &Expr) -> ZResult<crate::value::Value> {
        use crate::value::Value;
        match expr {
            Expr::Int(i) => Ok(Value::Int(*i)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Str(s) => Ok(Value::Str(std::rc::Rc::new(s.clone()))),
            Expr::None => Ok(Value::None),
            Expr::Tuple(t) => {
                let mut items = Vec::new();
                for e in t {
                    items.push(self.const_eval(e)?);
                }
                Ok(Value::Tuple(std::rc::Rc::new(items)))
            }
            Expr::List(l) => {
                let mut items = Vec::new();
                for e in l {
                    items.push(self.const_eval(e)?);
                }
                Ok(Value::List(std::rc::Rc::new(items)))
            }
            _ => Ok(Value::None),
        }
    }

    fn parse_enum(&mut self) -> ZResult<Stmt> {
        self.advance();
        let name = self.parse_target_name()?;
        self.expect(TokenKind::LBrace)?;
        let mut variants = Vec::new();
        self.skip_newlines();
        while !self.check(TokenKind::RBrace) && !self.is_eof() {
            let variant_name = self.parse_target_name()?;
            let value = if self.check(TokenKind::Assign) {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };
            variants.push((variant_name, value));
            self.end_statement()?;
        }
        self.expect(TokenKind::RBrace)?;
        Ok(Stmt::EnumDecl { name, variants })
    }

    fn parse_if(&mut self) -> ZResult<Stmt> {
        self.advance();
        let cond = self.parse_expr()?;
        let then_block = self.parse_block()?;
        let mut elif_blocks = Vec::new();
        while self.check(TokenKind::Elif) {
            self.advance();
            let c = self.parse_expr()?;
            let b = self.parse_block()?;
            elif_blocks.push((c, b));
        }
        let else_block = if self.check(TokenKind::Else) {
            self.advance();
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Stmt::If {
            cond,
            then_block,
            elif_blocks,
            else_block,
        })
    }

    fn parse_match(&mut self) -> ZResult<Stmt> {
        self.advance();
        let expr = self.parse_expr()?;
        self.expect(TokenKind::LBrace)?;
        let mut cases = Vec::new();
        self.skip_newlines();
        while !self.check(TokenKind::RBrace) && !self.is_eof() {
            self.expect(TokenKind::Case)?;
            let mut patterns = vec![self.parse_expr()?];
            while self.check(TokenKind::Comma) {
                self.advance();
                patterns.push(self.parse_expr()?);
            }
            let body = self.parse_block()?;
            cases.push(MatchCase { patterns, body });
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace)?;
        Ok(Stmt::Match { expr, cases })
    }

    fn parse_while(&mut self) -> ZResult<Stmt> {
        self.advance();
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::While { cond, body })
    }

    fn parse_for(&mut self) -> ZResult<Stmt> {
        self.advance();
        let mut targets = vec![self.parse_target_name()?];
        while self.check(TokenKind::Comma) {
            self.advance();
            targets.push(self.parse_target_name()?);
        }
        self.expect(TokenKind::In)?;
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::For {
            targets,
            iter,
            body,
        })
    }

    fn parse_try(&mut self) -> ZResult<Stmt> {
        self.advance();
        let body = self.parse_block()?;
        let mut excepts = Vec::new();
        while self.check(TokenKind::Except) {
            self.advance();
            let mut exc_types = Vec::new();
            let mut alias = None;
            if !self.check(TokenKind::LBrace) {
                let first = self.parse_target_name()?;
                exc_types.push(first);
                while self.check(TokenKind::Comma) {
                    self.advance();
                    let t = self.parse_target_name()?;
                    exc_types.push(t);
                }
                if self.check(TokenKind::As) {
                    self.advance();
                    alias = Some(self.parse_target_name()?);
                }
            }
            let handler_body = self.parse_block()?;
            excepts.push(ExceptHandler {
                exc_types,
                alias,
                body: handler_body,
            });
        }
        let else_block = if self.check(TokenKind::Else) {
            self.advance();
            Some(self.parse_block()?)
        } else {
            None
        };
        let finally_block = if self.check(TokenKind::Finally) {
            self.advance();
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Stmt::Try {
            body,
            excepts,
            else_block,
            finally_block,
        })
    }

    fn parse_with(&mut self) -> ZResult<Stmt> {
        self.advance();
        let expr = self.parse_expr()?;
        let alias = if self.check(TokenKind::As) {
            self.advance();
            Some(self.parse_target_name()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        Ok(Stmt::With { expr, alias, body })
    }

    fn parse_import(&mut self) -> ZResult<Stmt> {
        self.advance();
        let mut module = self.parse_target_name()?;
        while self.check(TokenKind::Dot) {
            self.advance();
            let part = self.parse_target_name()?;
            module = format!("{}.{}", module, part);
        }
        let alias = if self.check(TokenKind::As) {
            self.advance();
            Some(self.parse_target_name()?)
        } else {
            None
        };
        Ok(Stmt::Import { module, alias })
    }

    fn parse_from_import(&mut self) -> ZResult<Stmt> {
        self.advance();
        let module = self.parse_target_name()?;
        self.expect(TokenKind::Import)?;
        let names = self.parse_name_list()?;
        Ok(Stmt::FromImport { module, names })
    }

    fn parse_name_list(&mut self) -> ZResult<Vec<(String, Option<String>)>> {
        let mut names = Vec::new();
        loop {
            let name = self.parse_target_name()?;
            let alias = if self.check(TokenKind::As) {
                self.advance();
                Some(self.parse_target_name()?)
            } else {
                None
            };
            names.push((name, alias));
            if self.check(TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(names)
    }

    fn parse_dir_import(&mut self) -> ZResult<Stmt> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let path = match self.cur().clone() {
            TokenKind::String(s) => {
                self.advance();
                s
            }
            TokenKind::Ident(s) => {
                self.advance();
                s
            }
            _ => return Err(self.err("expected path in dir(...) import")),
        };
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::Import)?;
        let names = self.parse_name_list()?;
        Ok(Stmt::DirImport { path, names })
    }

    fn parse_return(&mut self) -> ZResult<Stmt> {
        self.advance();
        if self.is_statement_end() {
            Ok(Stmt::Return(None))
        } else {
            let mut exprs = vec![self.parse_expr()?];
            while self.check(TokenKind::Comma) {
                self.advance();
                exprs.push(self.parse_expr()?);
            }
            if exprs.len() == 1 {
                Ok(Stmt::Return(Some(exprs.into_iter().next().unwrap())))
            } else {
                Ok(Stmt::Return(Some(Expr::Tuple(exprs))))
            }
        }
    }

    fn parse_yield(&mut self) -> ZResult<Stmt> {
        self.advance();
        let expr = self.parse_expr()?;
        Ok(Stmt::Yield(expr))
    }

    fn parse_callerror(&mut self) -> ZResult<Stmt> {
        self.advance();
        if self.is_statement_end() {
            Ok(Stmt::CallError(None))
        } else {
            let expr = self.parse_expr()?;
            Ok(Stmt::CallError(Some(expr)))
        }
    }

    fn parse_assert(&mut self) -> ZResult<Stmt> {
        self.advance();
        let cond = self.parse_expr()?;
        let msg = if self.check(TokenKind::Comma) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        Ok(Stmt::Assert(cond, msg))
    }

    fn parse_del(&mut self) -> ZResult<Stmt> {
        self.advance();
        let name = match self.cur().clone() {
            TokenKind::Ident(s) => {
                self.advance();
                s
            }
            _ => return Err(self.err("expected name after del")),
        };
        Ok(Stmt::Del(name))
    }

    fn parse_global(&mut self) -> ZResult<Stmt> {
        self.advance();
        let mut names = vec![self.parse_target_name()?];
        while self.check(TokenKind::Comma) {
            self.advance();
            names.push(self.parse_target_name()?);
        }
        Ok(Stmt::Global(names))
    }

    fn parse_outer(&mut self) -> ZResult<Stmt> {
        self.advance();
        let mut names = vec![self.parse_target_name()?];
        while self.check(TokenKind::Comma) {
            self.advance();
            names.push(self.parse_target_name()?);
        }
        Ok(Stmt::Outer(names))
    }

    fn is_statement_end(&self) -> bool {
        matches!(
            self.cur(),
            TokenKind::Newline | TokenKind::Eof | TokenKind::RBrace
        )
    }

    fn skip_newlines(&mut self) {
        while matches!(self.cur(), TokenKind::Newline) {
            self.advance();
        }
    }

    // ---------- Expressions ----------

    fn parse_expr(&mut self) -> ZResult<Expr> {
        self.parse_ternary()
    }

    /// Parses a comprehension's iterable.
    ///
    /// A plain `parse_expr` would treat the comprehension's `if` as a ternary
    /// condition and demand a matching `else`, so the iterable stops one level
    /// lower: the `or` level, which excludes ternaries.
    fn parse_comp_iter(&mut self) -> ZResult<Expr> {
        self.parse_or()
    }

    fn parse_ternary(&mut self) -> ZResult<Expr> {
        let cond = self.parse_or()?;
        if self.check(TokenKind::If) {
            self.advance();
            let then_expr = self.parse_or()?;
            self.expect(TokenKind::Else)?;
            let else_expr = self.parse_ternary()?;
            Ok(Expr::Ternary {
                cond: Box::new(cond),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            })
        } else {
            Ok(cond)
        }
    }

    fn parse_or(&mut self) -> ZResult<Expr> {
        let mut left = self.parse_and()?;
        while self.check(TokenKind::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> ZResult<Expr> {
        let mut left = self.parse_not()?;
        while self.check(TokenKind::And) {
            self.advance();
            let right = self.parse_not()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> ZResult<Expr> {
        if self.check(TokenKind::Not) {
            self.advance();
            let expr = self.parse_not()?;
            Ok(Expr::UnaryOp {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            })
        } else {
            self.parse_comparison()
        }
    }

    fn parse_comparison(&mut self) -> ZResult<Expr> {
        let mut left = self.parse_bitor()?;
        loop {
            let op = match self.cur() {
                TokenKind::Eq => BinOp::Eq,
                TokenKind::NotEq => BinOp::NotEq,
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::LtEq => BinOp::LtEq,
                TokenKind::GtEq => BinOp::GtEq,
                TokenKind::In => BinOp::In,
                TokenKind::Not if self.peek_at(1) == &TokenKind::In => BinOp::NotIn,
                TokenKind::Is => BinOp::Is,
                _ => break,
            };
            match op {
                BinOp::NotIn => {
                    self.advance();
                    self.advance();
                    let right = self.parse_bitor()?;
                    left = mkbin(left, BinOp::NotIn, right);
                }
                BinOp::Is => {
                    self.advance();
                    if self.check(TokenKind::Not) {
                        self.advance();
                        let right = self.parse_bitor()?;
                        left = mkbin(left, BinOp::IsNot, right);
                    } else {
                        let right = self.parse_bitor()?;
                        left = mkbin(left, BinOp::Is, right);
                    }
                }
                _ => {
                    self.advance();
                    let right = self.parse_bitor()?;
                    left = mkbin(left, op, right);
                }
            }
        }
        Ok(left)
    }

    fn parse_bitor(&mut self) -> ZResult<Expr> {
        let mut left = self.parse_bitxor()?;
        while self.check(TokenKind::Pipe) {
            self.advance();
            let right = self.parse_bitxor()?;
            left = mkbin(left, BinOp::BitOr, right);
        }
        Ok(left)
    }

    fn parse_bitxor(&mut self) -> ZResult<Expr> {
        let mut left = self.parse_bitand()?;
        while self.check(TokenKind::Caret) {
            self.advance();
            let right = self.parse_bitand()?;
            left = mkbin(left, BinOp::BitXor, right);
        }
        Ok(left)
    }

    fn parse_bitand(&mut self) -> ZResult<Expr> {
        let mut left = self.parse_shift()?;
        while self.check(TokenKind::Amp) {
            self.advance();
            let right = self.parse_shift()?;
            left = mkbin(left, BinOp::BitAnd, right);
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> ZResult<Expr> {
        let mut left = self.parse_sum()?;
        loop {
            let op = match self.cur() {
                TokenKind::ShiftLeft => BinOp::ShiftLeft,
                TokenKind::ShiftRight => BinOp::ShiftRight,
                _ => break,
            };
            self.advance();
            let right = self.parse_sum()?;
            left = mkbin(left, op, right);
        }
        Ok(left)
    }

    fn parse_sum(&mut self) -> ZResult<Expr> {
        let mut left = self.parse_term()?;
        loop {
            let op = match self.cur() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_term()?;
            left = mkbin(left, op, right);
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> ZResult<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.cur() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::FloorDiv => BinOp::FloorDiv,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = mkbin(left, op, right);
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> ZResult<Expr> {
        match self.cur() {
            TokenKind::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                })
            }
            TokenKind::Plus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Pos,
                    expr: Box::new(expr),
                })
            }
            TokenKind::Tilde => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::BitNot,
                    expr: Box::new(expr),
                })
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> ZResult<Expr> {
        let base = self.parse_postfix()?;
        if self.check(TokenKind::Pow) {
            self.advance();
            let exp = self.parse_unary()?; // right associative, unary binds on the right
            Ok(mkbin(base, BinOp::Pow, exp))
        } else {
            Ok(base)
        }
    }

    fn parse_postfix(&mut self) -> ZResult<Expr> {
        if self.check(TokenKind::Call) || self.check(TokenKind::Callclass) {
            return self.parse_call_expr();
        }
        let mut expr = self.parse_primary()?;
        loop {
            match self.cur().clone() {
                TokenKind::Dot => {
                    self.advance();
                    let attr = self.parse_target_name()?;
                    expr = Expr::Attribute {
                        obj: Box::new(expr),
                        attr,
                    };
                }
                TokenKind::LBracket => {
                    self.advance();
                    let slice = self.parse_slice()?;
                    self.expect(TokenKind::RBracket)?;
                    expr = Expr::Index {
                        obj: Box::new(expr),
                        slice,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_call_expr(&mut self) -> ZResult<Expr> {
        let is_class = self.check(TokenKind::Callclass);
        self.advance(); // call | callclass
        let target = self.parse_call_target()?;
        let args = self.parse_call_args()?;
        let func = Expr::CallTarget {
            target: Box::new(target),
            is_class,
        };
        Ok(if is_class {
            Expr::CallClassExpr {
                func: Box::new(func),
                args,
            }
        } else {
            Expr::CallExpr {
                func: Box::new(func),
                args,
            }
        })
    }

    fn parse_call_target(&mut self) -> ZResult<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.cur().clone() {
                TokenKind::Dot => {
                    self.advance();
                    let attr = self.parse_target_name()?;
                    expr = Expr::Attribute {
                        obj: Box::new(expr),
                        attr,
                    };
                }
                TokenKind::LBracket => {
                    self.advance();
                    let slice = self.parse_slice()?;
                    self.expect(TokenKind::RBracket)?;
                    expr = Expr::Index {
                        obj: Box::new(expr),
                        slice,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_slice(&mut self) -> ZResult<Slice> {
        let first = if self.check(TokenKind::Colon) || self.check(TokenKind::RBracket) {
            None
        } else {
            Some(Box::new(self.parse_expr()?))
        };

        if self.check(TokenKind::Colon) {
            self.advance();
            let second = if self.check(TokenKind::Colon) || self.check(TokenKind::RBracket) {
                None
            } else {
                Some(Box::new(self.parse_expr()?))
            };
            let third = if self.check(TokenKind::Colon) {
                self.advance();
                if self.check(TokenKind::RBracket) {
                    None
                } else {
                    Some(Box::new(self.parse_expr()?))
                }
            } else {
                None
            };
            Ok(Slice::Full(first, second, third))
        } else if let Some(e) = first {
            Ok(Slice::Simple(e))
        } else {
            Err(self.err("invalid slice"))
        }
    }

    fn parse_call_args(&mut self) -> ZResult<Vec<Arg>> {
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        if self.check(TokenKind::RParen) {
            self.advance();
            return Ok(args);
        }
        loop {
            args.push(self.parse_arg()?);
            if self.check(TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(args)
    }

    fn parse_arg(&mut self) -> ZResult<Arg> {
        if self.check(TokenKind::Star) {
            self.advance();
            let value = self.parse_expr()?;
            return Ok(Arg {
                name: None,
                value,
                is_star: true,
                is_double_star: false,
            });
        }
        if self.check(TokenKind::Pow) {
            self.advance();
            let value = self.parse_expr()?;
            return Ok(Arg {
                name: None,
                value,
                is_star: false,
                is_double_star: true,
            });
        }
        if let TokenKind::Ident(id) = &self.cur().clone() {
            if self.peek_at(1) == &TokenKind::Assign {
                self.advance();
                self.advance();
                let value = self.parse_expr()?;
                return Ok(Arg {
                    name: Some(id.clone()),
                    value,
                    is_star: false,
                    is_double_star: false,
                });
            }
        }
        let value = self.parse_expr()?;
        Ok(Arg {
            name: None,
            value,
            is_star: false,
            is_double_star: false,
        })
    }

    fn parse_primary(&mut self) -> ZResult<Expr> {
        match self.cur().clone() {
            TokenKind::Int(i) => {
                self.advance();
                Ok(Expr::Int(i))
            }
            TokenKind::Float(f) => {
                self.advance();
                Ok(Expr::Float(f))
            }
            TokenKind::String(s) => {
                self.advance();
                Ok(Expr::Str(s))
            }
            TokenKind::FString(s) => {
                self.advance();
                self.parse_fstring(&s)
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            TokenKind::None => {
                self.advance();
                Ok(Expr::None)
            }
            TokenKind::Self_ => {
                self.advance();
                Ok(Expr::Ident("self".to_string()))
            }
            TokenKind::Super => {
                self.advance();
                Ok(Expr::Ident("super".to_string()))
            }
            TokenKind::Ident(id) => {
                self.advance();
                Ok(Expr::Ident(id))
            }
            TokenKind::LParen => {
                self.advance();
                if self.check(TokenKind::RParen) {
                    self.advance();
                    if self.check(TokenKind::Arrow) {
                        self.advance();
                        let body = self.parse_expr()?;
                        return Ok(Expr::Lambda {
                            params: Vec::new(),
                            body: Box::new(body),
                        });
                    }
                    return Ok(Expr::Tuple(Vec::new()));
                }
                let first = self.parse_expr()?;
                if self.check(TokenKind::Comma) {
                    let mut items = vec![first];
                    while self.check(TokenKind::Comma) {
                        self.advance();
                        if self.check(TokenKind::RParen) {
                            break;
                        }
                        items.push(self.parse_expr()?);
                    }
                    self.expect(TokenKind::RParen)?;
                    return Ok(Expr::Tuple(items));
                }
                self.expect(TokenKind::RParen)?;
                Ok(first)
            }
            TokenKind::Fn => {
                self.advance();
                let params = self.parse_params()?;
                self.expect(TokenKind::Arrow)?;
                let body = self.parse_expr()?;
                Ok(Expr::Lambda {
                    params,
                    body: Box::new(body),
                })
            }
            TokenKind::LBracket => self.parse_list_or_comprehension(),
            TokenKind::LBrace => self.parse_dict_or_set_or_comprehension(),
            TokenKind::Await => {
                self.advance();
                let inner = self.parse_expr()?;
                Ok(Expr::Await {
                    expr: Box::new(inner),
                })
            }
            TokenKind::Star => {
                self.advance();
                let expr = self.parse_expr()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Pos,
                    expr: Box::new(expr),
                })
            }
            _ => Err(self.err(&format!(
                "unexpected token {} in expression",
                tok_desc(&self.cur())
            ))),
        }
    }

    fn parse_list_or_comprehension(&mut self) -> ZResult<Expr> {
        self.advance(); // '['
        if self.check(TokenKind::RBracket) {
            self.advance();
            return Ok(Expr::List(Vec::new()));
        }
        let first = self.parse_expr()?;
        if self.check(TokenKind::For) {
            self.advance();
            let mut targets = vec![self.parse_target_name()?];
            while self.check(TokenKind::Comma) {
                self.advance();
                targets.push(self.parse_target_name()?);
            }
            self.expect(TokenKind::In)?;
            let iter = self.parse_comp_iter()?;
            let cond = if self.check(TokenKind::If) {
                self.advance();
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };
            self.expect(TokenKind::RBracket)?;
            return Ok(Expr::ListComp {
                expr: Box::new(first),
                target: targets.join(","),
                iter: Box::new(iter),
                cond,
            });
        }
        let mut items = vec![first];
        while self.check(TokenKind::Comma) {
            self.advance();
            if self.check(TokenKind::RBracket) {
                break;
            }
            items.push(self.parse_expr()?);
        }
        self.expect(TokenKind::RBracket)?;
        Ok(Expr::List(items))
    }

    fn parse_dict_or_set_or_comprehension(&mut self) -> ZResult<Expr> {
        self.advance(); // '{'
        if self.check(TokenKind::RBrace) {
            self.advance();
            return Ok(Expr::Dict(Vec::new()));
        }
        let first = self.parse_expr()?;
        if self.check(TokenKind::Colon) {
            self.advance();
            let value = self.parse_expr()?;
            if self.check(TokenKind::For) {
                self.advance();
                let mut targets = vec![self.parse_target_name()?];
                while self.check(TokenKind::Comma) {
                    self.advance();
                    targets.push(self.parse_target_name()?);
                }
                self.expect(TokenKind::In)?;
                let iter = self.parse_comp_iter()?;
                let cond = if self.check(TokenKind::If) {
                    self.advance();
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                self.expect(TokenKind::RBrace)?;
                return Ok(Expr::DictComp {
                    key: Box::new(first),
                    value: Box::new(value),
                    target: targets.join(","),
                    iter: Box::new(iter),
                    cond,
                });
            }
            let mut pairs = vec![(first, value)];
            while self.check(TokenKind::Comma) {
                self.advance();
                if self.check(TokenKind::RBrace) {
                    break;
                }
                let k = self.parse_expr()?;
                self.expect(TokenKind::Colon)?;
                let v = self.parse_expr()?;
                pairs.push((k, v));
            }
            self.expect(TokenKind::RBrace)?;
            Ok(Expr::Dict(pairs))
        } else {
            // A `{ ... }` without `:` is a set literal.
            let mut items = vec![first];
            while self.check(TokenKind::Comma) {
                self.advance();
                if self.check(TokenKind::RBrace) {
                    break;
                }
                items.push(self.parse_expr()?);
            }
            self.expect(TokenKind::RBrace)?;
            Ok(Expr::Set(items))
        }
    }

    fn parse_fstring(&self, raw: &str) -> ZResult<Expr> {
        let chars: Vec<char> = raw.chars().collect();
        let mut result: Option<Expr> = None;
        let mut lit = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '{' && i + 1 < chars.len() && chars[i + 1] == '{' {
                lit.push('{');
                i += 2;
                continue;
            }
            if c == '}' && i + 1 < chars.len() && chars[i + 1] == '}' {
                lit.push('}');
                i += 2;
                continue;
            }
            if c == '{' {
                if !lit.is_empty() {
                    let s = std::mem::take(&mut lit);
                    result = Some(append_expr(result, Expr::Str(s)));
                }
                let mut depth = 1;
                let mut expr_src = String::new();
                i += 1;
                while i < chars.len() && depth > 0 {
                    match chars[i] {
                        '{' => {
                            depth += 1;
                            expr_src.push('{');
                        }
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            expr_src.push('}');
                        }
                        other => expr_src.push(other),
                    }
                    i += 1;
                }
                i += 1; // skip closing brace
                let expr = parse_inline_expr(&expr_src)?;
                result = Some(append_expr(result, Expr::ToStr(Box::new(expr))));
                continue;
            }
            lit.push(c);
            i += 1;
        }
        if !lit.is_empty() {
            result = Some(append_expr(result, Expr::Str(lit)));
        }
        Ok(result.unwrap_or_else(|| Expr::Str(String::new())))
    }

    fn is_class_decl(&self) -> bool {
        if matches!(self.peek_at(1), TokenKind::LBrace) {
            return true;
        }
        if matches!(self.peek_at(1), TokenKind::LParen) {
            return self.parens_followed_by_brace(1);
        }
        false
    }

    /// True when the parenthesised group starting at `start` is followed by `{`,
    /// which is how a class with a base list (`Name(Base) {`) is told apart from
    /// an ordinary call expression (`Name(x)`).
    fn parens_followed_by_brace(&self, start: usize) -> bool {
        let mut depth: i32 = 0;
        let mut i = self.pos + start;
        // `start` is relative to the current token; scan for the group's `)`.
        let mut seen_open = false;
        while i < self.tokens.len() {
            match self.tokens[i].kind {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                    depth += 1;
                    seen_open = true;
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 && seen_open {
                        // The very next token decides: `{` means a class body.
                        return matches!(
                            self.tokens.get(i + 1).map(|t| &t.kind),
                            Some(TokenKind::LBrace)
                        );
                    }
                    if depth < 0 {
                        return false;
                    }
                }
                TokenKind::Newline | TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn cur(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_at(&self, n: usize) -> &TokenKind {
        let idx = (self.pos + n).min(self.tokens.len() - 1);
        &self.tokens[idx].kind
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.cur() == &kind
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
    }

    fn expect(&mut self, kind: TokenKind) -> ZResult<()> {
        if self.cur() == &kind {
            self.advance();
            Ok(())
        } else {
            Err(self.err(&format!(
                "expected {}, found {}",
                tok_desc(&kind),
                tok_desc(self.cur())
            )))
        }
    }

    fn is_eof(&self) -> bool {
        self.cur() == &TokenKind::Eof
    }

    fn err(&self, msg: &str) -> ZError {
        let tok = &self.tokens[self.pos];
        ZError::Parse(format!("line {}: {}", tok.line, msg))
    }
}

fn mkbin(left: Expr, op: BinOp, right: Expr) -> Expr {
    Expr::BinOp {
        left: Box::new(left),
        op,
        right: Box::new(right),
    }
}

fn append_expr(a: Option<Expr>, b: Expr) -> Expr {
    match a {
        None => b,
        Some(x) => mkbin(x, BinOp::Add, b),
    }
}

/// Parses a standalone expression used for f-string interpolation.
pub fn parse_inline_expr(src: &str) -> ZResult<Expr> {
    let mut lexer = crate::lexer::Lexer::new(src);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    parser.parse_expr()
}

fn tok_desc(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Ident(s) => format!("'{}'", s),
        TokenKind::Int(i) => format!("'{}'", i),
        TokenKind::Float(f) => format!("'{}'", f),
        TokenKind::String(s) => format!("'{}'", s),
        TokenKind::FString(s) => format!("f-string '{}'", s),
        TokenKind::Newline => "newline".to_string(),
        TokenKind::Eof => "end of file".to_string(),
        TokenKind::LParen => "'('".to_string(),
        TokenKind::RParen => "')'".to_string(),
        TokenKind::LBrace => "'{'".to_string(),
        TokenKind::RBrace => "'}'".to_string(),
        TokenKind::LBracket => "'['".to_string(),
        TokenKind::RBracket => "']'".to_string(),
        TokenKind::Comma => "','".to_string(),
        TokenKind::Colon => "':'".to_string(),
        TokenKind::Dot => "'.'".to_string(),
        TokenKind::Assign => "'='".to_string(),
        TokenKind::PlusEq => "'+='".to_string(),
        TokenKind::Arrow => "'=>'".to_string(),
        TokenKind::Abl => "'abl'".to_string(),
        TokenKind::Const => "'const'".to_string(),
        TokenKind::Fn => "'fn'".to_string(),
        TokenKind::Call => "'call'".to_string(),
        TokenKind::Callclass => "'callclass'".to_string(),
        TokenKind::Callerror => "'callerror'".to_string(),
        TokenKind::If => "'if'".to_string(),
        TokenKind::Elif => "'elif'".to_string(),
        TokenKind::Else => "'else'".to_string(),
        TokenKind::While => "'while'".to_string(),
        TokenKind::For => "'for'".to_string(),
        TokenKind::In => "'in'".to_string(),
        TokenKind::Return => "'return'".to_string(),
        TokenKind::Break => "'break'".to_string(),
        TokenKind::Continue => "'continue'".to_string(),
        TokenKind::Pass => "'pass'".to_string(),
        TokenKind::True => "'True'".to_string(),
        TokenKind::False => "'False'".to_string(),
        TokenKind::None => "'None'".to_string(),
        TokenKind::And => "'and'".to_string(),
        TokenKind::Or => "'or'".to_string(),
        TokenKind::Not => "'not'".to_string(),
        TokenKind::Is => "'is'".to_string(),
        TokenKind::Import => "'import'".to_string(),
        TokenKind::From => "'from'".to_string(),
        TokenKind::As => "'as'".to_string(),
        TokenKind::Try => "'try'".to_string(),
        TokenKind::Except => "'except'".to_string(),
        TokenKind::Finally => "'finally'".to_string(),
        TokenKind::Match => "'match'".to_string(),
        TokenKind::Case => "'case'".to_string(),
        TokenKind::Enum => "'enum'".to_string(),
        TokenKind::Del => "'del'".to_string(),
        TokenKind::Yield => "'yield'".to_string(),
        TokenKind::Async => "'async'".to_string(),
        TokenKind::Await => "'await'".to_string(),
        _ => "token".to_string(),
    }
}

fn is_type_name(s: &str) -> bool {
    matches!(
        s,
        "int" | "float" | "str" | "bool" | "list" | "dict" | "tuple" | "set"
    )
}