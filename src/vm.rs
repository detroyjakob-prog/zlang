//! The Z virtual machine: a tree-walking evaluator over the analyzed AST.

use crate::ast::{
    Arg, AssignOp, BinOp, ClassDecl, Expr, FunctionDecl, MatchCase, Slice, Stmt, Target, UnaryOp,
};
use crate::error::{ZError, ZResult};
use crate::value::{
    new_instance, BoundMethod, BuiltinFn, ClassDef, EnumDef, Function, InstanceRef, Map,
    Scope, Value, ValueKey,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// Result of executing a statement.
pub enum Flow {
    Next,
    Return(Value),
    Break,
    Continue,
}

const MAX_CALL_DEPTH: usize = 512;

pub struct Vm {
    pub globals: Rc<RefCell<HashMap<String, Value>>>,
    pub scope: Rc<RefCell<Scope>>,
    /// Currently executing (instance, class) pair, for `self` / `super`.
    self_stack: Vec<(InstanceRef, Rc<ClassDef>)>,
    depth: usize,
    loaded_modules: HashMap<String, Value>,
    pub base_dir: PathBuf,
    pub entry_dir: PathBuf,
    loading: Vec<PathBuf>,
    /// Values yielded by `yield` while running a generator function.
    pending_yields: Vec<Value>,
    current_error: Option<ZError>,
    error_retaken: bool,
    /// Seed for the `random` standard library module.
    pub rng_seed: Option<u64>,
    pub output: String,
    /// When true, `printer` writes to `output` instead of stdout (used in tests).
    pub capture_output: bool,
    /// Every live object, so mutating a field's container updates the owner
    /// even when the object is not the current `self`.
    pub instances: Vec<InstanceRef>,
}

impl Vm {
    pub fn new() -> Self {
        let globals = Rc::new(RefCell::new(HashMap::new()));
        let scope = Rc::new(RefCell::new(Scope::new(globals.clone())));
        let cwd = std::env::current_dir().unwrap_or_default();
        Vm {
            globals,
            scope,
            self_stack: Vec::new(),
            depth: 0,
            loaded_modules: HashMap::new(),
            base_dir: cwd.clone(),
            entry_dir: cwd,
            loading: Vec::new(),
            pending_yields: Vec::new(),
            current_error: None,
            error_retaken: false,
            rng_seed: None,
            output: String::new(),
            capture_output: false,
            instances: Vec::new(),
        }
    }

    /// Set the directory used to resolve relative imports.
    pub fn with_base_dir(mut self, dir: PathBuf) -> Self {
        self.base_dir = dir.clone();
        self.entry_dir = dir;
        self
    }

    /// Register the standard library and built-in functions.
    pub fn install_builtins(&mut self) {
        crate::builtins::install(self);
    }

    /// Run a compiled module.
    pub fn run(&mut self, module: &crate::compiler::Module) -> ZResult<()> {
        self.exec_block(&module.statements())?;
        self.run_main()?;
        Ok(())
    }

    /// Find and call a top-level `fn Main()` if the program defines one.
    fn run_main(&mut self) -> ZResult<()> {
        let main = self.globals.borrow().get("Main").cloned();
        if let Some(f @ Value::Function(_)) = main {
            self.call_value(f, Vec::new())?;
        }
        Ok(())
    }

    // ================= variables =================

    pub fn read_var(&self, name: &str) -> ZResult<Value> {
        if name == "self" || name == "super" {
            return self.read_special(name);
        }
        if let Some(v) = self.scope.borrow().lookup(name) {
            return Ok(v);
        }
        Err(ZError::raised(
            "NameError",
            format!("name '{}' is not defined", name),
        ))
    }

    pub fn read_var_opt(&self, name: &str) -> Option<Value> {
        if name == "self" || name == "super" {
            return self
                .self_stack
                .last()
                .map(|(inst, _)| match name {
                    "self" => Value::Instance(inst.clone()),
                    _ => Value::SuperProxy(inst.clone()),
                });
        }
        self.scope.borrow().lookup(name)
    }

    fn read_special(&self, name: &str) -> ZResult<Value> {
        match self.self_stack.last() {
            Some((inst, _)) => Ok(match name {
                "self" => Value::Instance(inst.clone()),
                _ => Value::SuperProxy(inst.clone()),
            }),
            None => Err(ZError::raised(
                "NameError",
                format!("name '{}' is not defined", name),
            )),
        }
    }

    pub fn write_var(&mut self, name: &str, value: Value) -> ZResult<()> {
        self.scope.borrow_mut().assign(name, value);
        Ok(())
    }

    /// Bind several names to values given as a tuple/list, e.g. `abl a, b = x`.
    fn bind_names(&mut self, names: &[String], values: Vec<Value>) -> ZResult<()> {
        if names.len() == 1 && values.len() == 1 {
            let (name, value) = (names[0].clone(), values.into_iter().next().unwrap());
            self.write_var(&name, value)?;
            return Ok(());
        }
        if values.len() != names.len() {
            // `abl first, *rest = [1, 2, 3, 4]` style unpacking.
            if names.len() >= 2 && names[names.len() - 1].starts_with('*') {
                let rest_name = names[names.len() - 1].trim_start_matches('*').to_string();
                let keep = names.len() - 1;
                if values.len() >= keep {
                    for (n, v) in names[..keep].iter().zip(values[..keep].iter()) {
                        self.write_var(n, v.clone())?;
                    }
                    let rest: Vec<Value> = values.into_iter().skip(keep).collect();
                    self.write_var(&rest_name, Value::list(rest))?;
                    return Ok(());
                }
            }
            return Err(ZError::raised(
                "ValueError",
                format!(
                    "cannot unpack {} values into {} targets",
                    values.len(),
                    names.len()
                ),
            ));
        }
        for (n, v) in names.iter().zip(values.into_iter()) {
            self.write_var(n, v)?;
        }
        Ok(())
    }

    // ================= assignment =================

    /// Execute `target op= value`, reading the current value first for `op != =`.
    fn exec_assign(&mut self, target: &Target, op: &AssignOp, value: &Expr) -> ZResult<()> {
        if matches!(*op, AssignOp::Assign) {
            let v = self.eval(value)?;
            return self.store_target(target, v);
        }
        let current = self.read_target(target)?;
        let rhs = self.eval(value)?;
        let binop = match op {
            AssignOp::Add => BinOp::Add,
            AssignOp::Sub => BinOp::Sub,
            AssignOp::Mul => BinOp::Mul,
            AssignOp::Div => BinOp::Div,
            AssignOp::FloorDiv => BinOp::FloorDiv,
            AssignOp::Mod => BinOp::Mod,
            AssignOp::Pow => BinOp::Pow,
            AssignOp::Assign => return self.store_target(target, rhs),
        };
        let new_value = self.apply_binop(&current, &binop, &rhs)?;
        self.store_target(target, new_value)
    }

    fn read_target(&mut self, target: &Target) -> ZResult<Value> {
        match target {
            Target::Name(n) | Target::Deref(n) => self.read_var(n),
            Target::Attribute(obj, attr) => {
                let o = self.eval(obj)?;
                self.attr_value(&o, attr)
            }
            Target::Index(obj, idx) => {
                let o = self.eval(obj)?;
                let i = self.eval(idx)?;
                self.get_item(&o, &i)
            }
        }
    }

    fn store_target(&mut self, target: &Target, value: Value) -> ZResult<()> {
        match target {
            Target::Name(n) | Target::Deref(n) => self.write_var(n, value),
            Target::Attribute(obj, attr) => {
                let o = self.eval(obj)?;
                self.set_attr(&o, attr, value)
            }
            Target::Index(obj, idx) => {
                let o = self.eval(obj)?;
                let i = self.eval(idx)?;
                self.set_item(&o, &i, value)
            }
        }
    }

    // ================= blocks =================

    pub fn exec_block(&mut self, stmts: &[Stmt]) -> ZResult<Option<Value>> {
        match self.exec_stmts(stmts)? {
            Flow::Return(v) => Ok(Some(v)),
            _ => Ok(None),
        }
    }

    fn exec_stmts(&mut self, stmts: &[Stmt]) -> ZResult<Flow> {
        for stmt in stmts {
            let flow = self.exec_stmt(stmt)?;
            if !matches!(flow, Flow::Next) {
                return Ok(flow);
            }
        }
        Ok(Flow::Next)
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> ZResult<Flow> {
        match stmt {
            Stmt::VarDecl { name, value, .. } => {
                let v = self.eval(value)?;
                self.write_var(name, v)?;
            }
            Stmt::MultiVarDecl { names, values } => {
                let vals = self.eval_multi(values)?;
                self.bind_names(names, vals)?;
            }
            Stmt::ConstDecl { name, value } => {
                let v = self.eval(value)?;
                self.globals.borrow_mut().insert(name.clone(), v.clone());
                // Constants defined at module level are exported from the module.
                if self.scope.borrow().parent.is_none() {
                    self.scope.borrow_mut().vars.insert(name.clone(), v);
                }
            }
            Stmt::Assign { target, op, value } => {
                self.exec_assign(target, op, value)?;
            }
            Stmt::MultiAssign { targets, values } => {
                let vals = self.eval_multi(values)?;
                if vals.len() != targets.len() {
                    return Err(ZError::raised(
                        "ValueError",
                        format!(
                            "cannot unpack {} values into {} targets",
                            vals.len(),
                            targets.len()
                        ),
                    ));
                }
                for (target, v) in targets.iter().zip(vals.into_iter()) {
                    self.store_target(target, v)?;
                }
            }
            Stmt::FnDecl(decl) => {
                let f = self.make_function(decl)?;
                self.scope.borrow_mut().assign(&decl.name, f.clone());
                // Top-level functions are also global so `Main` and imports find them.
                if self.scope.borrow().parent.is_none() {
                    self.globals.borrow_mut().insert(decl.name.clone(), f);
                }
            }
            Stmt::ClassDecl(decl) => {
                self.eval_class(decl)?;
                if self.scope.borrow().parent.is_none() {
                    if let Some(v) = self.scope.borrow().vars.get(&decl.name).cloned() {
                        self.globals.borrow_mut().insert(decl.name.clone(), v);
                    }
                }
            }
            Stmt::EnumDecl { name, variants } => {
                let mut def = EnumDef {
                    name: name.clone(),
                    variants: Vec::new(),
                };
                for (vname, vexpr) in variants {
                    let v = match vexpr {
                        Some(e) => self.eval(e)?,
                        None => Value::Int(def.variants.len() as i64),
                    };
                    def.variants.push((vname.clone(), v));
                }
                self.scope.borrow_mut().assign(name, Value::Enum(Rc::new(def)));
            }
            Stmt::If {
                cond,
                then_block,
                elif_blocks,
                else_block,
            } => {
                if self.eval(cond)?.is_truthy() {
                    return self.exec_stmts(then_block);
                }
                for (ec, eb) in elif_blocks {
                    if self.eval(ec)?.is_truthy() {
                        return self.exec_stmts(eb);
                    }
                }
                if let Some(eb) = else_block {
                    return self.exec_stmts(eb);
                }
            }
            Stmt::Match { expr, cases } => {
                return self.exec_match(expr, cases);
            }
            Stmt::While { cond, body } => loop {
                if !self.eval(cond)?.is_truthy() {
                    break;
                }
                match self.exec_stmts(body)? {
                    Flow::Break => break,
                    Flow::Continue => continue,
                    Flow::Return(v) => return Ok(Flow::Return(v)),
                    Flow::Next => {}
                }
            },
            Stmt::For {
                targets,
                iter,
                body,
            } => {
                let it = self.eval(iter)?;
                let items = self.iterate(&it)?;
                for item in items {
                    let unpacked = unfold(item, targets.len())?;
                    self.bind_names(targets, unpacked)?;
                    match self.exec_stmts(body)? {
                        Flow::Break => break,
                        Flow::Continue => continue,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Next => {}
                    }
                }
            }
            Stmt::Try {
                body,
                excepts,
                else_block,
                finally_block,
            } => {
                return self.exec_try(body, excepts, else_block, finally_block);
            }
            Stmt::With { expr, alias, body } => {
                let v = self.eval(expr)?;
                if let Some(a) = alias {
                    self.write_var(a, v)?;
                }
                return self.exec_stmts(body);
            }
            Stmt::Return(e) => {
                let v = match e {
                    Some(e) => self.eval(e)?,
                    None => Value::None,
                };
                return Ok(Flow::Return(v));
            }
            Stmt::Yield(e) => {
                let v = self.eval(e)?;
                self.pending_yields.push(v);
            }
            Stmt::Expr(e) => {
                self.eval(e)?;
            }
            Stmt::CallError(e) => {
                let v = match e {
                    None => return Err(self.reraise()),
                    Some(e) => self.eval(e)?,
                };
                return Err(self.to_zerror(&v));
            }
            Stmt::Assert(cond, msg) => {
                let c = self.eval(cond)?;
                if !c.is_truthy() {
                    let text = match msg {
                        Some(m) => self.eval(m)?,
                        None => Value::str("assertion failed"),
                    };
                    let m = self.to_string_value(&text);
                    return Err(ZError::raised("AssertionError", m));
                }
            }
            Stmt::Del(name) => {
                self.scope.borrow_mut().vars.remove(name);
            }
            Stmt::Global(names) => {
                let mut s = self.scope.borrow_mut();
                for n in names {
                    if !s.global_names.contains(n) {
                        s.global_names.push(n.clone());
                    }
                }
            }
            Stmt::Outer(names) => {
                let mut s = self.scope.borrow_mut();
                for n in names {
                    if !s.outer_names.contains(n) {
                        s.outer_names.push(n.clone());
                    }
                }
            }
            Stmt::Import { module, alias } => {
                let v = self.load_module(module)?;
                let bind = alias.clone().unwrap_or_else(|| module.clone());
                self.globals.borrow_mut().insert(bind, v);
            }
            Stmt::FromImport { module, names } => {
                let m = self.load_module(module)?;
                for (n, a) in names {
                    let v = self.attr_value(&m, n)?;
                    let bind = a.clone().unwrap_or_else(|| n.clone());
                    self.scope.borrow_mut().vars.insert(bind, v);
                }
            }
            Stmt::DirImport { path, names } => {
                let m = self.load_dir_module(path)?;
                for (n, a) in names {
                    let v = self.attr_value(&m, n)?;
                    let bind = a.clone().unwrap_or_else(|| n.clone());
                    self.scope.borrow_mut().vars.insert(bind, v);
                }
            }
            Stmt::Break => return Ok(Flow::Break),
            Stmt::Continue => return Ok(Flow::Continue),
            Stmt::Pass => {}
        }
        Ok(Flow::Next)
    }

    fn exec_match(&mut self, expr: &Expr, cases: &[MatchCase]) -> ZResult<Flow> {
        let subject = self.eval(expr)?;
        for case in cases {
            for pattern in &case.patterns {
                let p = self.eval(pattern)?;
                if is_wildcard_value(&p) || values_equal(&p, &subject) {
                    return self.exec_stmts(&case.body);
                }
            }
        }
        Ok(Flow::Next)
    }

    // ================= try / except =================

    fn exec_try(
        &mut self,
        body: &[Stmt],
        excepts: &[crate::ast::ExceptHandler],
        else_block: &Option<Vec<Stmt>>,
        finally_block: &Option<Vec<Stmt>>,
    ) -> ZResult<Flow> {
        let outcome: ZResult<Flow> = match self.exec_stmts(body) {
            Ok(Flow::Return(v)) => {
                if let Some(eb) = else_block {
                    self.exec_stmts(eb)?;
                }
                Ok(Flow::Return(v))
            }
            Ok(flow) => {
                if let Some(eb) = else_block {
                    if let Flow::Return(v) = self.exec_stmts(eb)? {
                        return self.run_finally(finally_block, Flow::Return(v));
                    }
                }
                Ok(flow)
            }
            Err(err) => match self.handle_error(err, excepts)? {
                Some(flow) => Ok(flow),
                None => Ok(Flow::Next),
            },
        };
        self.run_finally(finally_block, outcome.unwrap_or(Flow::Next))
    }

    fn run_finally(
        &mut self,
        finally_block: &Option<Vec<Stmt>>,
        outcome: Flow,
    ) -> ZResult<Flow> {
        if let Some(fb) = finally_block {
            match self.exec_stmts(fb)? {
                Flow::Next => {}
                other => return Ok(other),
            }
        }
        Ok(outcome)
    }

    /// Returns `Some(flow)` when an except clause handled the error.
    fn handle_error(
        &mut self,
        err: ZError,
        excepts: &[crate::ast::ExceptHandler],
    ) -> ZResult<Option<Flow>> {
        let raised = to_raised(&err);

        for handler in excepts {
            let matches = handler.exc_types.is_empty()
                || handler
                    .exc_types
                    .iter()
                    .any(|t| match_error(&raised, t, self));
            if !matches {
                continue;
            }
            self.current_error = Some(err.clone());
            self.error_retaken = false;
            let bound = match &raised.instance {
                Some(inst) => inst.clone(),
                None => Value::str(raised.message.clone()),
            };
            if let Some(alias) = &handler.alias {
                self.scope.borrow_mut().vars.insert(alias.clone(), bound);
            }
            let flow = self.exec_stmts(&handler.body)?;
            if self.error_retaken {
                return Err(self.reraise());
            }
            return Ok(Some(flow));
        }
        Err(err)
    }

    fn reraise(&self) -> ZError {
        self.current_error
            .clone()
            .unwrap_or_else(|| ZError::raised("RuntimeError", "no active error to re-raise"))
    }

    fn to_zerror(&mut self, v: &Value) -> ZError {
        match v {
            Value::Instance(i) => {
                let class = i.borrow().class.clone();
                let raw = i.borrow().fields.get("message").cloned();
                let message = match &raw {
                    Some(m) => self.to_string_value(m),
                    None => String::new(),
                };
                ZError::raise_instance(Value::Instance(i.clone()), class.name.clone(), message)
            }
            Value::Class(c) => ZError::raised(c.name.clone(), c.name.clone()),
            Value::Str(s) => ZError::raised("Error", (**s).clone()),
            other => ZError::raised("Error", self.to_string_value(other)),
        }
    }
}

/// Convert any error into a catchable descriptor.
fn to_raised(err: &ZError) -> crate::error::RaisedError {
    match err {
        ZError::Raised(r) => r.clone(),
        other => crate::error::RaisedError {
            class_name: other.error_class().to_string(),
            message: other.report(),
            instance: None,
        },
    }
}

fn match_error(raised: &crate::error::RaisedError, name: &str, vm: &Vm) -> bool {
    if raised.class_name == name {
        return true;
    }
    if let Some(Value::Instance(i)) = &raised.instance {
        let mut cur = Some(i.borrow().class.clone());
        while let Some(c) = cur {
            if c.name == name {
                return true;
            }
            cur = vm.parent_class(&c);
        }
    }
    false
}

/// `_` in a match pattern matches anything.
fn is_wildcard_value(v: &Value) -> bool {
    matches!(v, Value::Str(s) if s.as_str() == "_")
}

/// Split `value` into `count` parts for `for a, b in ...` / tuple assignment.
fn unfold(value: Value, count: usize) -> ZResult<Vec<Value>> {
    if count <= 1 {
        return Ok(vec![value]);
    }
    match value {
        Value::Tuple(v) | Value::List(v) => {
            if v.len() != count {
                return Err(ZError::raised(
                    "ValueError",
                    format!("cannot unpack {} values into {} targets", v.len(), count),
                ));
            }
            Ok(v.iter().cloned().collect())
        }
        other => Err(ZError::raised(
            "ValueError",
            format!("cannot unpack {} into {} targets", other.type_name(), count),
        )),
    }
}

/// Structural equality used by `==` and by `match` patterns.
pub fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::None, Value::None) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Int(x), Value::Float(y)) => (*x as f64) == *y,
        (Value::Float(x), Value::Int(y)) => *x == (*y as f64),
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::List(x), Value::List(y)) | (Value::Tuple(x), Value::Tuple(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| values_equal(p, q))
        }
        (Value::List(x), Value::Tuple(y)) | (Value::Tuple(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| values_equal(p, q))
        }
        (Value::Set(x), Value::Set(y)) => {
            x.len() == y.len() && x.iter().all(|e| y.iter().any(|o| values_equal(e, o)))
        }
        (Value::Set(x), Value::List(y)) | (Value::Set(x), Value::Tuple(y)) => {
            x.len() == y.len() && x.iter().all(|e| y.iter().any(|o| values_equal(e, o)))
        }
        (Value::Dict(x), Value::Dict(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|o| values_equal(v, o)))
        }
        (Value::Instance(x), Value::Instance(y)) => {
            if Rc::ptr_eq(x, y) {
                return true;
            }
            // Use `eq` if either class defines it.
            if x.borrow().class.get_instance_method("eq").is_some()
                || y.borrow().class.get_instance_method("eq").is_some()
            {
                // Handled by the caller through operator overloading; fall back to equality.
                return x.borrow().fields == y.borrow().fields;
            }
            false
        }
        (Value::Function(_), Value::Function(_)) => false,
        _ => false,
    }
}
// ================= expression evaluation =================

impl Vm {
    pub fn eval(&mut self, expr: &Expr) -> ZResult<Value> {
        match expr {
            Expr::None => Ok(Value::None),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Int(i) => Ok(Value::Int(*i)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Str(s) => Ok(Value::str(s.clone())),
            Expr::Ident(name) => self.read_var(name),
            Expr::ToStr(inner) => {
                let v = self.eval(inner)?;
                Ok(Value::str(self.to_string_value(&v)))
            }
            Expr::BinOp { left, op, right } => self.eval_binop(left, op, right),
            Expr::UnaryOp { op, expr } => {
                let v = self.eval(expr)?;
                self.eval_unary(op, &v)
            }
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
            } => {
                if self.eval(cond)?.is_truthy() {
                    self.eval(then_expr)
                } else {
                    self.eval(else_expr)
                }
            }
            Expr::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for e in items {
                    out.push(self.eval(e)?);
                }
                Ok(Value::list(out))
            }
            Expr::Tuple(items) => {
                let mut out = Vec::with_capacity(items.len());
                for e in items {
                    out.push(self.eval(e)?);
                }
                Ok(Value::tuple(out))
            }
            Expr::Set(items) => {
                let mut out: Vec<Value> = Vec::new();
                for e in items {
                    let v = self.eval(e)?;
                    if !out.iter().any(|o| values_equal(o, &v)) {
                        out.push(v);
                    }
                }
                Ok(Value::set(out))
            }
            Expr::Dict(pairs) => {
                let mut map = Map::new();
                for (k, v) in pairs {
                    let key = self.eval(k)?;
                    let val = self.eval(v)?;
                    self.map_set(&mut map, key, val)?;
                }
                Ok(Value::dict(map))
            }
            Expr::Attribute { obj, attr } => {
                let o = self.eval(obj)?;
                self.attr_value(&o, attr)
            }
            Expr::Index { obj, slice } => {
                let o = self.eval(obj)?;
                match slice {
                    Slice::Simple(idx) => {
                        let i = self.eval(idx)?;
                        self.get_item(&o, &i)
                    }
                    Slice::Full(a, b, c) => {
                        let av = self.eval_opt(a)?;
                        let bv = self.eval_opt(b)?;
                        let cv = self.eval_opt(c)?;
                        self.get_slice(&o, av.as_ref(), bv.as_ref(), cv.as_ref())
                    }
                }
            }
            Expr::Lambda { params, body } => {
                let decl = FunctionDecl {
                    name: "<lambda>".to_string(),
                    params: params.clone(),
                    body: vec![Stmt::Return(Some((**body).clone()))],
                    is_async: false,
                    is_static: false,
                    return_type: None,
                };
                self.make_function(&decl)
            }
            Expr::CallTarget { target, is_class } => {
                // `call f` / `callclass C.m` used as a value (rare) resolves the target.
                let v = self.eval(target)?;
                if *is_class {
                    self.callclass_target(target, &v, &[])
                } else {
                    Ok(v)
                }
            }
            Expr::CallExpr { func, args } => self.eval_call(func, args),
            Expr::CallClassExpr { func, args } => self.eval_callclass(func, args),
            Expr::Await { expr } => self.eval(expr),
            Expr::ListComp {
                expr,
                target,
                iter,
                cond,
            } => self.eval_list_comp(expr, target, iter, cond.as_deref()),
            Expr::DictComp {
                key,
                value,
                target,
                iter,
                cond,
            } => self.eval_dict_comp(key, value, target, iter, cond.as_deref()),
        }
    }

    fn eval_opt(&mut self, e: &Option<Box<Expr>>) -> ZResult<Option<Value>> {
        match e {
            Some(e) => Ok(Some(self.eval(e)?)),
            None => Ok(None),
        }
    }

    fn eval_multi(&mut self, exprs: &[Expr]) -> ZResult<Vec<Value>> {
        let mut out = Vec::with_capacity(exprs.len());
        for e in exprs {
            out.push(self.eval(e)?);
        }
        Ok(out)
    }

    // ---------------- calls ----------------

    fn eval_call(&mut self, func: &Expr, args: &[Arg]) -> ZResult<Value> {
        let (receiver, callee, name) = self.resolve_callable(func)?;
        let args = self.eval_args(args)?;
        match (receiver, callee) {
            (Some(_), Some(method)) => self.call_value(method, args),
            (None, Some(method)) => self.call_value(method, args),
            (Some(recv), None) => {
                // A method on a builtin container / module namespace.
                self.method_call(&recv, &name, args)
            }
            (None, None) => Err(ZError::raised(
                "NameError",
                format!("name '{}' is not defined", name),
            )),
        }
    }

    /// `callclass expr.member(args)`.
    ///
    /// `expr.member` is always treated as a member access on a receiver, so a
    /// method of a class, a built-in container method, or a module function all
    /// follow the same path. A bare `callclass Name(args)` constructs a class.
    fn eval_callclass(&mut self, func: &Expr, args: &[Arg]) -> ZResult<Value> {
        let args = self.eval_args(args)?;
        match func {
            Expr::CallTarget { target, .. } => match &**target {
                // `callclass Name(args)` — construct an instance.
                Expr::Ident(_) => {
                    let v = self.eval(target)?;
                    match v {
                        Value::Class(c) => self.construct(&c, args),
                        Value::Builtin(b) => (b.func)(self, &args, &Map::new()),
                        Value::BoundMethod(b) => self.call_bound(&b, args),
                        Value::Function(f) => self.call_value(Value::Function(f), args),
                        other => Err(ZError::raised(
                            "TypeError",
                            format!("'{}' is not callable", other.type_name()),
                        )),
                    }
                }
                // `callclass expr.member(args)` — dispatch as a method.
                Expr::Attribute { obj, attr } => {
                    let recv = self.eval(obj)?;
                    self.method_call(&recv, attr, args)
                }
                _ => {
                    let v = self.eval(target)?;
                    match v {
                        Value::Class(c) => self.construct(&c, args),
                        other => self.call_value(other, args),
                    }
                }
            },
            // Direct call of a value: `callclass Foo(x)()` style.
            other => {
                let v = self.eval(other)?;
                match v {
                    Value::Class(c) => self.construct(&c, args),
                    other => self.call_value(other, args),
                }
            }
        }
    }

    /// Resolve a `call`/`callclass` target into `(receiver, direct callee, name)`.
    ///
    /// `Ident("len")` gives `(None, Some(builtin_len), "len")`.
    /// `user.get` gives `(Some(user_value), <class method>, "get")`.
    /// `Tools.Square` gives `(Some(class_value), <static method>, "Square")`.
    fn resolve_callable(&mut self, func: &Expr) -> ZResult<(Option<Value>, Option<Value>, String)> {
        match func {
            Expr::CallTarget { target, .. } => self.resolve_callable(target),
            Expr::Ident(name) => {
                if let Some(v) = self.read_var_opt(name) {
                    return Ok((None, Some(v), name.clone()));
                }
                Ok((None, None, name.clone()))
            }
            Expr::Attribute { obj, attr } => {
                let base = self.eval(obj)?;
                // Class-level field of an object/class, e.g. `super.init`.
                if let Some(v) = self.attr_value_opt(&base, attr) {
                    return Ok((None, Some(v), attr.clone()));
                }
                Ok((Some(base), None, attr.clone()))
            }
            Expr::Index { obj, slice } => {
                let base = self.eval(obj)?;
                if let Slice::Simple(idx) = slice {
                    let i = self.eval(idx)?;
                    let v = self.get_item(&base, &i)?;
                    return Ok((None, Some(v), "<index>".to_string()));
                }
                Ok((None, None, "<index>".to_string()))
            }
            other => {
                let v = self.eval(other)?;
                Ok((None, Some(v), "<value>".to_string()))
            }
        }
    }

    fn callclass_target(
        &mut self,
        target: &Expr,
        value: &Value,
        args: &[Value],
    ) -> ZResult<Value> {
        match target {
            Expr::Attribute { obj, attr } => {
                let base = self.eval(obj)?;
                if let Some(v) = self.attr_value_opt(&base, attr) {
                    return self.call_value(v, args.to_vec());
                }
                self.method_call(&base, attr, args.to_vec())
            }
            _ => self.call_value(value.clone(), args.to_vec()),
        }
    }

    pub fn eval_args(&mut self, args: &[Arg]) -> ZResult<Vec<Value>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            let v = self.eval(&a.value)?;
            if a.is_star {
                // Splat: expand the value into the argument list.
                for item in self.iterate(&v)? {
                    out.push(item);
                }
            } else if a.is_double_star {
                // Ignored for now: keyword arguments are evaluated positionally.
            } else {
                out.push(v);
            }
        }
        Ok(out)
    }

    fn call_bound(&mut self, bound: &BoundMethod, args: Vec<Value>) -> ZResult<Value> {
        self.self_stack
            .push((bound.instance.clone(), bound.class.clone()));
        let result = self.invoke(&bound.func, Some(bound.instance.clone()), args);
        self.self_stack.pop();
        result
    }

    /// Call a function value with positional arguments.
    pub fn call_value(&mut self, callable: Value, args: Vec<Value>) -> ZResult<Value> {
        match callable {
            Value::Function(f) => {
                self.enter()?;
                let r = self.invoke(&f, None, args);
                self.leave();
                r
            }
            Value::Builtin(b) => {
                let kwargs = Map::new();
                (b.func)(self, &args, &kwargs)
            }
            Value::BoundMethod(b) => self.call_bound(&b, args),
            Value::Class(c) => self.construct(&c, args),
            _ => Err(ZError::raised(
                "TypeError",
                format!("'{}' object is not callable", callable.type_name()),
            )),
        }
    }

    /// Call a builtin with an explicit keyword-argument map.
    pub fn call_builtin(&mut self, b: &Rc<BuiltinFn>, args: &[Value], kwargs: &Map) -> ZResult<Value> {
        (b.func)(self, args, kwargs)
    }

    fn enter(&mut self) -> ZResult<()> {
        self.depth += 1;
        if self.depth > MAX_CALL_DEPTH {
            self.depth -= 1;
            return Err(ZError::raised(
                "RecursionError",
                "maximum recursion depth exceeded",
            ));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn invoke(
        &mut self,
        f: &Rc<Function>,
        self_value: Option<InstanceRef>,
        args: Vec<Value>,
    ) -> ZResult<Value> {
        crate::compiler::check_params(&f.params)?;
        let mut scope = self.make_call_scope(f);
        self.bind_params(&mut scope, f, args, self_value.clone())?;
        let is_generator = f.is_generator;
        // Publish `self` and `super` for the duration of the call.
        let self_frame = self_value.as_ref().map(|inst| {
            let class = inst.borrow().class.clone();
            (inst.clone(), class)
        });
        if let Some(frame) = self_frame.clone() {
            self.self_stack.push(frame);
        }
        let saved = self.scope.clone();
        self.scope = Rc::new(RefCell::new(scope));
        if is_generator {
            self.pending_yields.clear();
        }
        let result = self.exec_stmts(&f.body);
        let generated = std::mem::take(&mut self.pending_yields);
        self.scope = saved;
        if self_frame.is_some() {
            self.self_stack.pop();
        }
        match result {
            Ok(Flow::Return(v)) => Ok(if is_generator { Value::list(generated) } else { v }),
            Ok(_) => Ok(if is_generator {
                Value::list(generated)
            } else {
                Value::None
            }),
            Err(e) => Err(e),
        }
    }

    fn make_call_scope(&self, f: &Rc<Function>) -> Scope {
        match &f.closure {
            Some(parent) => Scope::child(parent.clone()),
            None => Scope::new(self.globals.clone()),
        }
    }

    fn bind_params(
        &mut self,
        scope: &mut Scope,
        f: &Rc<Function>,
        args: Vec<Value>,
        self_value: Option<InstanceRef>,
    ) -> ZResult<()> {
        let params = f.params.as_slice();
        // An implicit leading `self` for instance methods.
        let mut positional: Vec<Value> = Vec::new();
        if let Some(inst) = self_value {
            if params
                .first()
                .map(|p| p.name == "self")
                .unwrap_or(true)
            {
                positional.push(Value::Instance(inst.clone()));
            }
        }
        positional.extend(args);

        let mut idx = 0usize;
        for (i, p) in params.iter().enumerate() {
            if p.is_vararg || p.is_kwarg {
                break;
            }
            if idx < positional.len() {
                scope.vars.insert(p.name.clone(), positional[idx].clone());
                idx += 1;
            } else if let Some(d) = &p.default {
                scope.vars.insert(p.name.clone(), self.eval(d)?);
            } else {
                return Err(ZError::raised(
                    "TypeError",
                    format!("missing required argument '{}'", p.name),
                ));
            }
            let _ = i;
        }
        // Collect *args and **kwargs.
        for p in params.iter() {
            if p.is_vararg {
                let rest = positional[idx.min(positional.len())..].to_vec();
                scope.vars.insert(p.name.clone(), Value::tuple(rest));
                idx = positional.len();
            }
        }
        for p in params.iter() {
            if p.is_kwarg {
                let mut m = Map::new();
                while idx < positional.len() {
                    if let Some(k) = &positional[idx].as_key() {
                        let key = k.clone();
                        let val = positional[idx + 1].clone();
                        m.insert(key, val);
                        idx += 2;
                    } else {
                        idx += 1;
                    }
                }
                scope.vars.insert(p.name.clone(), Value::dict(m));
            }
        }
        if idx < positional.len() {
            return Err(ZError::raised(
                "TypeError",
                format!(
                    "{}() takes {} argument(s) but {} were given",
                    f.name,
                    f.params.len(),
                    positional.len()
                ),
            ));
        }
        Ok(())
    }

    /// Build a function value for `decl` capturing the current scope.
    pub fn make_function(&mut self, decl: &FunctionDecl) -> ZResult<Value> {
        crate::compiler::check_params(&decl.params)?;
        let f = Function {
            name: decl.name.clone(),
            params: decl.params.clone(),
            body: decl.body.clone(),
            is_async: decl.is_async,
            is_static: decl.is_static,
            closure: Some(self.scope.clone()),
            return_type: decl.return_type.clone(),
            is_generator: crate::compiler::is_generator(&decl.body),
        };
        Ok(Value::Function(Rc::new(f)))
    }
}

// ================= classes and objects =================

impl Vm {
    pub fn eval_class(&mut self, decl: &ClassDecl) -> ZResult<()> {
        // Build the class, then register it so methods can reference it.
        let mut def = ClassDef {
            name: decl.name.clone(),
            parent: decl.parent.clone(),
            methods: HashMap::new(),
            class_methods: HashMap::new(),
            fields: HashMap::new(),
        };

        let class_rc = Rc::new(def.clone());
        self.scope
            .borrow_mut()
            .assign(&decl.name, Value::Class(class_rc.clone()));

        let mut members: Vec<(&str, Rc<Function>, bool)> = Vec::new();
        for member in &decl.members {
            match member {
                crate::ast::ClassMember::Method {
                    name,
                    params,
                    body,
                    is_static,
                    return_type,
                } => {
                    let inner = FunctionDecl {
                        name: name.clone(),
                        params: params.clone(),
                        body: body.clone(),
                        is_async: false,
                        is_static: *is_static,
                        return_type: return_type.clone(),
                    };
                    let f = self.make_function(&inner)?;
                    if let Value::Function(f) = f {
                        members.push((name, f, *is_static));
                    }
                }
                crate::ast::ClassMember::Field { name, value } => {
                    def.fields.insert(name.clone(), value.clone());
                }
            }
        }

        for (name, f, is_static) in &members {
            // `static fn` and methods with no `self` parameter are class-level.
            let has_self = f.params.first().map(|p| p.name == "self").unwrap_or(false);
            if *is_static || !has_self {
                def.class_methods.insert(name.to_string(), f.clone());
            } else {
                def.methods.insert(name.to_string(), f.clone());
            }
        }

        let final_rc = Rc::new(def);
        self.scope
            .borrow_mut()
            .assign(&decl.name, Value::Class(final_rc.clone()));
        // Expose class-level constants such as `Tools.MAX` via globals for scripts.
        for (k, v) in final_rc.fields.iter() {
            self.globals
                .borrow_mut()
                .insert(format!("{}.{}", decl.name, k), v.clone());
        }
        Ok(())
    }

    fn parent_class(&self, class: &ClassDef) -> Option<Rc<ClassDef>> {
        match &class.parent {
            Some(p) => match self.globals.borrow().get(p) {
                Some(Value::Class(c)) => Some(c.clone()),
                _ => None,
            },
            None => None,
        }
    }

    /// Construct an instance, running `init` when present.
    pub fn construct(&mut self, class: &Rc<ClassDef>, args: Vec<Value>) -> ZResult<Value> {
        let inst = new_instance(class.clone());
        self.instances.push(inst.clone());
        if let Some(init) = self.find_method(class, "init") {
            self.self_stack.push((inst.clone(), class.clone()));
            let r = self.invoke(&init, Some(inst.clone()), args);
            self.self_stack.pop();
            r?;
        } else if !args.is_empty() {
            return Err(ZError::raised(
                "TypeError",
                format!("{}() takes no arguments", class.name),
            ));
        }
        Ok(Value::Instance(inst))
    }

    fn find_method(&self, class: &Rc<ClassDef>, name: &str) -> Option<Rc<Function>> {
        let mut cur = Some(class.clone());
        while let Some(c) = cur {
            if let Some(m) = c.get_instance_method(name) {
                return Some(m);
            }
            cur = self.parent_class(&c);
        }
        None
    }

    fn find_class_method(&self, class: &Rc<ClassDef>, name: &str) -> Option<Rc<Function>> {
        let mut cur = Some(class.clone());
        while let Some(c) = cur {
            if let Some(m) = c.get_class_method(name) {
                return Some(m);
            }
            cur = self.parent_class(&c);
        }
        None
    }

    // ---------------- attribute access ----------------

    pub fn attr_value(&mut self, obj: &Value, attr: &str) -> ZResult<Value> {
        match self.attr_value_opt(obj, attr) {
            Some(v) => Ok(v),
            None => match obj {
                Value::Instance(i) => Err(ZError::raised(
                    "AttributeError",
                    format!("'{}' object has no attribute '{}'", i.borrow().class.name, attr),
                )),
                Value::Class(c) => Err(ZError::raised(
                    "AttributeError",
                    format!("class '{}' has no attribute '{}'", c.name, attr),
                )),
                Value::Enum(e) => match e.variants.iter().find(|(n, _)| n == attr) {
                    Some((_, v)) => Ok(v.clone()),
                    None => Err(ZError::raised(
                        "AttributeError",
                        format!("enum '{}' has no member '{}'", e.name, attr),
                    )),
                },
                Value::SuperProxy(inst) => {
                    let class = inst.borrow().class.clone();
                    let parent = self.parent_class(&class).ok_or_else(|| {
                        ZError::raised("RuntimeError", "'super' has no parent class")
                    })?;
                    self.attr_value(&Value::Class(parent), attr)
                }
                Value::Builtin(b) => match b.members.get(attr) {
                    Some(v) => Ok(v.clone()),
                    None => Err(ZError::raised(
                        "AttributeError",
                        format!("'{}' has no attribute '{}'", b.name, attr),
                    )),
                },
                other => match self.builtin_getattr(other, attr) {
                    Some(v) => Ok(v),
                    None => Err(self.attr_error(other, attr)),
                },
            },
        }
    }

    fn attr_value_opt(&self, obj: &Value, attr: &str) -> Option<Value> {
        match obj {
            // Modules are dict namespaces: `tools.Helper` and `tools.Tools.Main`.
            Value::Dict(d) => d.get(&ValueKey::Str(attr.to_string())).cloned(),
            Value::Instance(i) => {
                if let Some(v) = i.borrow().fields.get(attr) {
                    return Some(v.clone());
                }
                let class = i.borrow().class.clone();
                if let Some(v) = class.fields.get(attr) {
                    return Some(v.clone());
                }
                if let Some(f) = class.get_instance_method(attr) {
                    return Some(Value::BoundMethod(BoundMethod {
                        instance: i.clone(),
                        class: class.clone(),
                        func: f,
                    }));
                }
                None
            }
            Value::Class(c) => {
                if let Some(v) = c.fields.get(attr) {
                    return Some(v.clone());
                }
                if let Some(f) = c.get_class_method(attr) {
                    return Some(Value::Function(f));
                }
                None
            }
            Value::Enum(e) => e
                .variants
                .iter()
                .find(|(n, _)| n == attr)
                .map(|(_, v)| v.clone()),
            Value::SuperProxy(inst) => {
                let class = inst.borrow().class.clone();
                let parent = self.parent_class(&class)?;
                parent.fields.get(attr).cloned().or_else(|| {
                    parent
                        .get_instance_method(attr)
                        .map(|f| Value::BoundMethod(BoundMethod {
                            instance: inst.clone(),
                            class: parent.clone(),
                            func: f,
                        }))
                })
            }
            Value::Builtin(b) => b.members.get(attr).cloned(),
            _ => None,
        }
    }

    pub fn set_attr(&mut self, obj: &Value, attr: &str, value: Value) -> ZResult<()> {
        match obj {
            Value::Instance(i) => {
                i.borrow_mut().fields.insert(attr.to_string(), value);
                Ok(())
            }
            Value::SuperProxy(inst) => {
                inst.borrow_mut().fields.insert(attr.to_string(), value);
                Ok(())
            }
            Value::Dict(_) => {
                let mut m = match obj {
                    Value::Dict(d) => (**d).clone(),
                    _ => unreachable!(),
                };
                let key = Value::str(attr);
                self.map_set(&mut m, key, value)?;
                Ok(())
            }
            Value::Class(_) => Err(ZError::raised(
                "AttributeError",
                "cannot set attributes on a class",
            )),
            other => match self.builtin_getattr(other, attr) {
                Some(_) => Err(ZError::raised(
                    "AttributeError",
                    format!("cannot set attribute '{}' on {}", attr, other.type_name()),
                )),
                None => Err(ZError::raised(
                    "AttributeError",
                    format!("'{}' object has no attribute '{}'", other.type_name(), attr),
                )),
            },
        }
    }
}

// ================= indexing and iteration =================

impl Vm {
    /// `obj[key]` with support for instance `get`.
    pub fn get_item(&mut self, obj: &Value, key: &Value) -> ZResult<Value> {
        match obj {
            Value::List(l) => {
                let idx = self.as_index(key, l.len(), "list")?;
                Ok(l[idx].clone())
            }
            Value::Tuple(t) => {
                let idx = self.as_index(key, t.len(), "tuple")?;
                Ok(t[idx].clone())
            }
            Value::Str(s) => {
                let len = s.chars().count();
                let idx = self.as_index(key, len, "str")?;
                let c = s.chars().nth(idx).unwrap();
                Ok(Value::str(c.to_string()))
            }
            Value::Dict(d) => match key.as_key() {
                Some(k) => match d.get(&k) {
                    Some(v) => Ok(v.clone()),
                    None => Err(ZError::raised("KeyError", self.to_string_value(key))),
                },
                None => Err(ZError::raised(
                    "TypeError",
                    "unhashable type used as a dict key",
                )),
            },
            Value::Instance(i) => {
                if i.borrow().class.get_instance_method("get").is_some() {
                    return self.method_call(obj, "get", vec![key.clone()]);
                }
                Err(ZError::raised(
                    "TypeError",
                    format!("'{}' object is not subscriptable", i.borrow().class.name),
                ))
            }
            Value::Set(s) => {
                let idx = self.as_index(key, s.len(), "set")?;
                Ok(s[idx].clone())
            }
            other => Err(ZError::raised(
                "TypeError",
                format!("'{}' object is not subscriptable", other.type_name()),
            )),
        }
    }

    fn as_index(&mut self, key: &Value, len: usize, ctx: &str) -> ZResult<usize> {
        let n = match key {
            Value::Int(i) => *i,
            Value::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            Value::Float(f) => {
                if f.fract() != 0.0 {
                    return Err(ZError::raised("TypeError", "indices must be integers"));
                }
                *f as i64
            }
            other => {
                return Err(ZError::raised(
                    "TypeError",
                    format!("{} indices must be integers, not {}", ctx, other.type_name()),
                ))
            }
        };
        let idx = if n < 0 { n + len as i64 } else { n };
        if idx < 0 || idx >= len as i64 {
            return Err(ZError::raised("IndexError", self.to_string_value(key)));
        }
        Ok(idx as usize)
    }

    fn slice_bound(&mut self, v: Option<&Value>, len: usize) -> ZResult<Option<usize>> {
        match v {
            None => Ok(None),
            Some(Value::None) => Ok(None),
            Some(Value::Int(i)) => Ok(Some(normalize_slice_end(*i, len))),
            Some(Value::Float(f)) => Ok(Some(normalize_slice_end(*f as i64, len))),
            Some(other) => Err(ZError::raised(
                "TypeError",
                format!("slice indices must be integers, not {}", other.type_name()),
            )),
        }
    }

    pub fn get_slice(
        &mut self,
        obj: &Value,
        a: Option<&Value>,
        b: Option<&Value>,
        c: Option<&Value>,
    ) -> ZResult<Value> {
        if matches!(obj, Value::Instance(_)) {
            return self.method_call(obj, "get", vec![Value::None]);
        }
        let len = self.len(obj)?;
        let start = self.slice_bound(a, len)?.unwrap_or(0);
        let stop = self.slice_bound(b, len)?.unwrap_or(len);
        let (start, stop) = if start <= stop { (start, stop) } else { (start, stop) };
        let items: Vec<Value> = match obj {
            Value::List(l) => l[start..stop.min(len)].to_vec(),
            Value::Tuple(t) => t[start..stop.min(len)].to_vec(),
            Value::Set(s) => s[start..stop.min(len)].to_vec(),
            Value::Str(s) => s
                .chars()
                .skip(start)
                .take(stop.saturating_sub(start))
                .map(|c| Value::str(c.to_string()))
                .collect(),
            Value::Dict(_) => Vec::new(),
            other => {
                return Err(ZError::raised(
                    "TypeError",
                    format!("'{}' object is not subscriptable", other.type_name()),
                ))
            }
        };
        let _ = c;
        if matches!(obj, Value::Str(_)) {
            let joined: String = items
                .iter()
                .map(|v| self.to_string_value(v))
                .collect::<Vec<_>>()
                .join("");
            Ok(Value::str(joined))
        } else {
            Ok(match obj {
                Value::Tuple(_) => Value::tuple(items),
                Value::Set(_) => Value::set(items),
                _ => Value::list(items),
            })
        }
    }

    pub fn set_item(&mut self, obj: &Value, key: &Value, value: Value) -> ZResult<()> {
        match obj {
            Value::List(l) => {
                let len = l.len();
                let idx = self.as_index(key, len, "list")?;
                let mut v = (**l).clone();
                v[idx] = value;
                self.replace_container(obj, Value::list(v))?;
                Ok(())
            }
            Value::Dict(d) => {
                let mut m = (**d).clone();
                self.map_set(&mut m, key.clone(), value)?;
                self.replace_container(obj, Value::dict(m))?;
                Ok(())
            }
            Value::Set(s) => {
                let mut items = (**s).clone();
                let idx = match key {
                    Value::Int(i) => *i as usize,
                    Value::None => 0,
                    _ => items.len(),
                };
                if idx < items.len() {
                    items[idx] = value;
                } else {
                    items.push(value);
                }
                self.replace_container(obj, Value::set(items))?;
                Ok(())
            }
            Value::Instance(i) => {
                if i.borrow().class.get_instance_method("set").is_some() {
                    self.method_call(obj, "set", vec![key.clone(), value])?;
                }
                Ok(())
            }
            other => Err(ZError::raised(
                "TypeError",
                format!("'{}' object does not support item assignment", other.type_name()),
            )),
        }
    }

    pub fn map_set(&mut self, map: &mut Map, key: Value, value: Value) -> ZResult<()> {
        match key.as_key() {
            Some(k) => {
                map.insert(k, value);
                Ok(())
            }
            None => Err(ZError::raised(
                "TypeError",
                format!("unhashable type '{}'", key.type_name()),
            )),
        }
    }

    /// Iterate a value into a vector, honouring instance `iter`.
    pub fn iterate(&mut self, value: &Value) -> ZResult<Vec<Value>> {
        match value {
            Value::List(l) => Ok((**l).clone()),
            Value::Tuple(t) => Ok((**t).clone()),
            Value::Set(s) => Ok((**s).clone()),
            Value::Str(s) => Ok(s.chars().map(|c| Value::str(c.to_string())).collect()),
            Value::Dict(d) => Ok(d.keys().map(|k| k.to_value()).collect()),
            Value::Instance(i) => {
                if i.borrow().class.get_instance_method("iter").is_some() {
                    let res = self.method_call(value, "iter", Vec::new())?;
                    return self.iterate(&res);
                }
                if i.borrow().class.get_instance_method("next").is_some() {
                    // Pull values until an exception is raised.
                    let mut out = Vec::new();
                    loop {
                        match self.method_call(value, "next", Vec::new()) {
                            Ok(v) => out.push(v),
                            Err(_) => break,
                        }
                    }
                    return Ok(out);
                }
                Err(ZError::raised(
                    "TypeError",
                    format!("'{}' object is not iterable", i.borrow().class.name),
                ))
            }
            Value::None => Ok(Vec::new()),
            other => Err(ZError::raised(
                "TypeError",
                format!("'{}' object is not iterable", other.type_name()),
            )),
        }
    }

    /// `len(x)` for any value that supports it.
    pub fn len(&mut self, value: &Value) -> ZResult<usize> {
        match value {
            Value::Str(s) => Ok(s.chars().count()),
            Value::List(l) => Ok(l.len()),
            Value::Tuple(t) => Ok(t.len()),
            Value::Dict(d) => Ok(d.len()),
            Value::Set(s) => Ok(s.len()),
            Value::Instance(i) => {
                if i.borrow().class.get_instance_method("len").is_some() {
                    let v = self.method_call(value, "len", Vec::new())?;
                    return match v {
                        Value::Int(n) => Ok(n as usize),
                        _ => Ok(0),
                    };
                }
                Err(ZError::raised(
                    "TypeError",
                    format!("object of type '{}' has no len()", i.borrow().class.name),
                ))
            }
            other => Err(ZError::raised(
                "TypeError",
                format!("object of type '{}' has no len()", other.type_name()),
            )),
        }
    }

    pub fn contains(&mut self, obj: &Value, item: &Value) -> ZResult<bool> {
        match obj {
            Value::Str(s) => {
                let needle = self.to_string_value(item);
                Ok(s.contains(&needle))
            }
            Value::List(l) | Value::Tuple(l) => {
                Ok(l.iter().any(|e| values_equal(e, item)))
            }
            Value::Set(s) => Ok(s.iter().any(|e| values_equal(e, item))),
            Value::Dict(d) => match item.as_key() {
                Some(k) => Ok(d.contains_key(&k)),
                None => Ok(false),
            },
            Value::Instance(_) => {
                let items = self.iterate(obj)?;
                Ok(items.iter().any(|e| values_equal(e, item)))
            }
            other => Err(ZError::raised(
                "TypeError",
                format!("argument of type '{}' is not iterable", other.type_name()),
            )),
        }
    }
}

fn normalize_slice_end(i: i64, len: usize) -> usize {
    if i < 0 {
        (i + len as i64).max(0) as usize
    } else {
        (i as usize).min(len)
    }
}

// ================= operators =================

impl Vm {
    fn eval_binop(&mut self, left: &Expr, op: &BinOp, right: &Expr) -> ZResult<Value> {
        match op {
            BinOp::And => {
                let l = self.eval(left)?;
                if !l.is_truthy() {
                    return Ok(l);
                }
                self.eval(right)
            }
            BinOp::Or => {
                let l = self.eval(left)?;
                if l.is_truthy() {
                    return Ok(l);
                }
                self.eval(right)
            }
            _ => {
                let l = self.eval(left)?;
                let r = self.eval(right)?;
                self.apply_binop(&l, op, &r)
            }
        }
    }

    pub fn apply_binop(&mut self, l: &Value, op: &BinOp, r: &Value) -> ZResult<Value> {
        // Instance operator overloading, e.g. `add`, `eq`, `lt`.
        if let Some(v) = self.try_operator_method(l, op, r)? {
            return Ok(v);
        }
        match op {
            BinOp::Add => self.add(l, r),
            BinOp::Sub => self.arith(l, r, '-'),
            BinOp::Mul => self.mul(l, r),
            BinOp::Div => {
                self.check_numeric(l, r, "/")?;
                Ok(Value::Float(l.as_float() / r.as_float()))
            }
            BinOp::FloorDiv => {
                self.check_numeric(l, r, "//")?;
                if let Value::Int(_) = r {
                    if r.as_float() == 0.0 {
                        return Err(ZError::raised("ZeroDivisionError", "division by zero"));
                    }
                }
                Ok(Value::Int((l.as_float() / r.as_float()).floor() as i64))
            }
            BinOp::Mod => self.modulo(l, r),
            BinOp::Pow => {
                self.check_numeric(l, r, "**")?;
                Ok(from_float(l.as_float().powf(r.as_float())))
            }
            BinOp::BitAnd => self.bitop(l, r, '&'),
            BinOp::BitOr => self.bitop(l, r, '|'),
            BinOp::BitXor => self.bitop(l, r, '^'),
            BinOp::ShiftLeft => self.shift(l, r, false),
            BinOp::ShiftRight => self.shift(l, r, true),
            BinOp::Eq => Ok(Value::Bool(values_equal(l, r))),
            BinOp::NotEq => Ok(Value::Bool(!values_equal(l, r))),
            BinOp::Lt => Ok(Value::Bool(self.compare(l, r)? == std::cmp::Ordering::Less)),
            BinOp::Gt => Ok(Value::Bool(self.compare(l, r)? == std::cmp::Ordering::Greater)),
            BinOp::LtEq => Ok(Value::Bool(self.compare(l, r)? != std::cmp::Ordering::Greater)),
            BinOp::GtEq => Ok(Value::Bool(self.compare(l, r)? != std::cmp::Ordering::Less)),
            BinOp::In => Ok(Value::Bool(self.contains(r, l)?)),
            BinOp::NotIn => Ok(Value::Bool(!self.contains(r, l)?)),
            BinOp::Is => Ok(Value::Bool(self.is_same(l, r))),
            BinOp::IsNot => Ok(Value::Bool(!self.is_same(l, r))),
            BinOp::And | BinOp::Or => unreachable!("handled in eval_binop"),
        }
    }

    fn try_operator_method(
        &mut self,
        l: &Value,
        op: &BinOp,
        r: &Value,
    ) -> ZResult<Option<Value>> {
        let method = match op {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Div => "div",
            BinOp::FloorDiv => "floordiv",
            BinOp::Mod => "mod",
            BinOp::Pow => "pow",
            BinOp::Eq => "eq",
            BinOp::Lt => "lt",
            BinOp::Gt => "gt",
            BinOp::LtEq => "le",
            BinOp::GtEq => "ge",
            _ => return Ok(None),
        };
        if let Value::Instance(i) = l {
            if self.attr_value_opt(l, method).is_some() {
                let v = self.method_call(l, method, vec![r.clone()])?;
                return Ok(Some(v));
            }
            let _ = i;
        }
        Ok(None)
    }

    fn add(&mut self, l: &Value, r: &Value) -> ZResult<Value> {
        match (l, r) {
            (Value::Float(_), _) | (_, Value::Float(_)) => {
                Ok(Value::Float(l.as_float() + r.as_float()))
            }
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Str(a), Value::Str(b)) => {
                Ok(Value::str(format!("{}{}", a, b)))
            }
            (Value::List(a), Value::List(b)) => {
                let mut out = (**a).clone();
                out.extend((**b).iter().cloned());
                Ok(Value::list(out))
            }
            (Value::List(a), Value::Tuple(b)) => {
                let mut out = (**a).clone();
                out.extend((**b).iter().cloned());
                Ok(Value::list(out))
            }
            (Value::Tuple(a), Value::Tuple(b)) => {
                let mut out = (**a).clone();
                out.extend((**b).iter().cloned());
                Ok(Value::tuple(out))
            }
            (Value::Tuple(a), Value::List(b)) => {
                let mut out = (**a).clone();
                out.extend((**b).iter().cloned());
                Ok(Value::tuple(out))
            }
            (Value::Set(a), Value::Set(b)) => {
                let mut out: Vec<Value> = (**a).clone();
                for v in b.iter() {
                    if !out.iter().any(|o| values_equal(o, v)) {
                        out.push(v.clone());
                    }
                }
                Ok(Value::set(out))
            }
            (Value::List(a), Value::Set(b)) => {
                let mut out = (**a).clone();
                for v in b.iter() {
                    if !out.iter().any(|o| values_equal(o, v)) {
                        out.push(v.clone());
                    }
                }
                Ok(Value::list(out))
            }
            (Value::Dict(a), Value::Dict(b)) => {
                let mut m = (**a).clone();
                for (k, v) in b.iter() {
                    m.insert(k.clone(), v.clone());
                }
                Ok(Value::dict(m))
            }
            _ => Err(self.type_error("+", l, r)),
        }
    }

    fn mul(&mut self, l: &Value, r: &Value) -> ZResult<Value> {
        match (l, r) {
            (Value::Str(s), Value::Int(n)) | (Value::Int(n), Value::Str(s)) => {
                if n <= &0 {
                    Ok(Value::str(""))
                } else {
                    let mut out = String::new();
                    for _ in 0..n.abs() {
                        out.push_str(s);
                    }
                    Ok(Value::str(out))
                }
            }
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (Value::List(a), Value::Int(n)) | (Value::Int(n), Value::List(a)) => {
                let mut out = Vec::new();
                for _ in 0..(*n).max(0) {
                    out.extend(a.iter().cloned());
                }
                Ok(Value::list(out))
            }
            (Value::Float(_), _) | (_, Value::Float(_)) => {
                Ok(from_float(l.as_float() * r.as_float()))
            }
            (Value::Set(a), Value::Set(b)) => {
                let out: Vec<Value> = a
                    .iter()
                    .filter(|v| b.iter().any(|o| values_equal(v, o)))
                    .cloned()
                    .collect();
                Ok(Value::set(out))
            }
            _ => Err(self.type_error("*", l, r)),
        }
    }

    fn arith(&mut self, l: &Value, r: &Value, op: char) -> ZResult<Value> {
        self.check_numeric(l, r, &op.to_string())?;
        let (a, b) = (l.as_float(), r.as_float());
        let res = match op {
            '-' => a - b,
            _ => unreachable!(),
        };
        Ok(match (l, r) {
            (Value::Int(_), Value::Int(_)) => Value::Int(res as i64),
            _ => Value::Float(res),
        })
    }

    fn modulo(&mut self, l: &Value, r: &Value) -> ZResult<Value> {
        self.check_numeric(l, r, "%")?;
        if r.as_float() == 0.0 {
            return Err(ZError::raised("ZeroDivisionError", "integer division or modulo by zero"));
        }
        let res = l.as_float() % r.as_float();
        Ok(match (l, r) {
            (Value::Int(_), Value::Int(_)) => Value::Int(res as i64),
            _ => Value::Float(res),
        })
    }

    pub fn bitop(&mut self, l: &Value, r: &Value, op: char) -> ZResult<Value> {
        let (a, b) = (self.as_int(l, "bitwise")?, self.as_int(r, "bitwise")?);
        Ok(Value::Int(match op {
            '&' => a & b,
            '|' => a | b,
            '^' => a ^ b,
            _ => unreachable!(),
        }))
    }

    fn shift(&mut self, l: &Value, r: &Value, right: bool) -> ZResult<Value> {
        let (a, b) = (self.as_int(l, "shift")?, self.as_int(r, "shift")?);
        if right {
            Ok(Value::Int(a >> b.max(0)))
        } else {
            Ok(Value::Int(a << b.max(0).min(62)))
        }
    }

    fn as_int(&mut self, v: &Value, ctx: &str) -> ZResult<i64> {
        match v {
            Value::Int(i) => Ok(*i),
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            other => Err(ZError::raised(
                "TypeError",
                format!("unsupported operand type for {}: '{}'", ctx, other.type_name()),
            )),
        }
    }

    fn check_numeric(&self, l: &Value, r: &Value, op: &str) -> ZResult<()> {
        let ok = |v: &Value| matches!(v, Value::Int(_) | Value::Float(_) | Value::Bool(_));
        if ok(l) && ok(r) {
            Ok(())
        } else {
            Err(self.type_error(op, l, r))
        }
    }

    fn type_error(&self, op: &str, l: &Value, r: &Value) -> ZError {
        ZError::raised(
            "TypeError",
            format!(
                "unsupported operand type(s) for {}: '{}' and '{}'",
                op,
                l.type_name(),
                r.type_name()
            ),
        )
    }

    fn compare(&mut self, l: &Value, r: &Value) -> ZResult<std::cmp::Ordering> {
        if let Some(v) = self.try_compare_method(l, r)? {
            return Ok(v);
        }
        let numeric = |v: &Value| matches!(v, Value::Int(_) | Value::Float(_) | Value::Bool(_));
        if numeric(l) && numeric(r) {
            return Ok(l
                .as_float()
                .partial_cmp(&r.as_float())
                .unwrap_or(std::cmp::Ordering::Equal));
        }
        if let (Value::Str(a), Value::Str(b)) = (l, r) {
            return Ok(a.cmp(b));
        }
        match (l, r) {
            (Value::List(a) | Value::Tuple(a), Value::List(b) | Value::Tuple(b)) => {
                for (x, y) in a.iter().zip(b.iter()) {
                    let ord = self.compare(x, y)?;
                    if ord != std::cmp::Ordering::Equal {
                        return Ok(ord);
                    }
                }
                Ok(a.len().cmp(&b.len()))
            }
            (Value::Set(a), Value::Set(b)) => Ok(a.len().cmp(&b.len())),
            _ => match values_equal(l, r) {
                true => Ok(std::cmp::Ordering::Equal),
                false => Err(ZError::raised(
                    "TypeError",
                    format!(
                        "'<' not supported between instances of '{}' and '{}'",
                        l.type_name(),
                        r.type_name()
                    ),
                )),
            },
        }
    }

    fn try_compare_method(&mut self, l: &Value, r: &Value) -> ZResult<Option<std::cmp::Ordering>> {
        if let Value::Instance(_) = l {
            if self.attr_value_opt(l, "lt").is_some() {
                let v = self.method_call(l, "lt", vec![r.clone()])?;
                if let Value::Bool(b) = v {
                    return Ok(Some(if b {
                        std::cmp::Ordering::Less
                    } else if values_equal(l, r) {
                        std::cmp::Ordering::Equal
                    } else {
                        std::cmp::Ordering::Greater
                    }));
                }
            }
        }
        Ok(None)
    }

    fn is_same(&self, l: &Value, r: &Value) -> bool {
        match (l, r) {
            (Value::None, Value::None) => true,
            (Value::Instance(a), Value::Instance(b)) => Rc::ptr_eq(a, b),
            (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
            (Value::Dict(a), Value::Dict(b)) => Rc::ptr_eq(a, b),
            (Value::Set(a), Value::Set(b)) => Rc::ptr_eq(a, b),
            (Value::Tuple(a), Value::Tuple(b)) => Rc::ptr_eq(a, b),
            (Value::Function(_), Value::Function(_)) => false,
            (Value::Builtin(_), Value::Builtin(_)) => false,
            _ => values_equal(l, r),
        }
    }

    fn eval_unary(&mut self, op: &UnaryOp, v: &Value) -> ZResult<Value> {
        match op {
            UnaryOp::Neg => match v {
                Value::Int(i) => Ok(Value::Int(-i)),
                Value::Float(f) => Ok(Value::Float(-f)),
                other => Err(ZError::raised(
                    "TypeError",
                    format!("bad operand type for unary -: '{}'", other.type_name()),
                )),
            },
            UnaryOp::Pos => Ok(v.clone()),
            UnaryOp::Not => Ok(Value::Bool(!v.is_truthy())),
            UnaryOp::BitNot => match v {
                Value::Int(i) => Ok(Value::Int(!i)),
                other => Err(ZError::raised(
                    "TypeError",
                    format!("bad operand type for unary ~: '{}'", other.type_name()),
                )),
            },
        }
    }
}

/// Build a value that keeps `int` integral and everything else float.
pub fn from_float(f: f64) -> Value {
    if f.fract() == 0.0 && f.is_finite() && f.abs() < 9.007_199_254_740_992e15 {
        let i = f as i64;
        if i as f64 == f {
            return Value::Int(i);
        }
    }
    Value::Float(f)
}

fn normalize_float(fl: f64) -> Value {
    from_float(fl)
}

pub fn use_from_float(f: f64) -> Value {
    normalize_float(f)
}

// ================= method calls on built-ins =================

impl Vm {
    /// `callclass recv.name(args)` / `callclass "s".upper()`.
    pub fn method_call(&mut self, obj: &Value, name: &str, args: Vec<Value>) -> ZResult<Value> {
        // Instances dispatch through their class.
        if let Some(inst) = obj.as_instance() {
            let class = inst.borrow().class.clone();
            if let Some(f) = self.find_method(&class, name) {
                return self.call_bound(
                    &BoundMethod {
                        instance: inst.clone(),
                        class,
                        func: f,
                    },
                    args,
                );
            }
            if let Some(v) = crate::value::instance_get(inst, name) {
                return self.call_value(v, args);
            }
            return Err(ZError::raised(
                "AttributeError",
                format!("'{}' object has no method '{}'", class.name, name),
            ));
        }

        // Classes support `callclass C.method()` for static methods and fields.
        if let Value::Class(c) = obj {
            if let Some(f) = self.find_class_method(c, name) {
                return self.call_value(Value::Function(f), args);
            }
            if let Some(f) = c.get_instance_method(name) {
                // A class-level call on an instance method: pass a placeholder.
                let inst = new_instance(c.clone());
                return self.call_bound(
                    &BoundMethod {
                        instance: inst,
                        class: c.clone(),
                        func: f,
                    },
                    args,
                );
            }
            if let Some(v) = c.fields.get(name) {
                return self.call_value(v.clone(), args);
            }
            return Err(ZError::raised(
                "AttributeError",
                format!("class '{}' has no method '{}'", c.name, name),
            ));
        }

        // `super.init(...)` style calls.
        if let Value::SuperProxy(inst) = obj {
            let class = inst.borrow().class.clone();
            let parent = self.parent_class(&class).ok_or_else(|| {
                ZError::raised("RuntimeError", "'super' object has no parent class")
            })?;
            if let Some(f) = self.find_method(&parent, name) {
                self.self_stack.push((inst.clone(), parent.clone()));
                let r = self.call_bound(
                    &BoundMethod {
                        instance: inst.clone(),
                        class: parent,
                        func: f,
                    },
                    args,
                );
                self.self_stack.pop();
                return r;
            }
            return Err(ZError::raised(
                "AttributeError",
                format!("parent class has no method '{}'", name),
            ));
        }

        // A dict holding a member of that name is a module namespace:
        // `call tools.Helper()`. Otherwise it is an ordinary dict and the
        // method (get, keys, ...) belongs to the dict itself.
        if let Value::Dict(d) = obj {
            let key = ValueKey::Str(name.to_string());
            if let Some(Value::Builtin(_)) | Some(Value::Function(_)) | Some(Value::Class(_)) = d.get(&key)
            {
                return self.call_value(d.get(&key).unwrap().clone(), args);
            }
        }

        crate::builtins::builtin_method(self, obj, name, args)
    }

    fn attr_error(&self, obj: &Value, attr: &str) -> ZError {
        attr_error_of(obj, attr)
    }

    fn builtin_getattr(&self, obj: &Value, attr: &str) -> Option<Value> {
        crate::builtins::builtin_attr(obj, attr)
    }

    /// Render a value the way `call str(x)` does.
    pub fn to_string_value(&mut self, v: &Value) -> String {
        self.str_repr(v, false)
    }

    /// Render a value the way `printer(...)` shows it inside collections.
    pub fn repr_value(&mut self, v: &Value) -> String {
        self.str_repr(v, true)
    }

    fn str_repr(&mut self, v: &Value, nested: bool) -> String {
        match v {
            Value::Str(s) => {
                if nested {
                    format!("'{}'", s)
                } else {
                    s.to_string()
                }
            }
            Value::None => "None".to_string(),
            Value::Bool(b) => if *b { "True".to_string() } else { "False".to_string() },
            Value::Int(i) => i.to_string(),
            Value::Float(f) => format_float(*f),
            Value::List(l) | Value::Tuple(l) | Value::Set(l) => {
                let mut inner: Vec<String> = Vec::with_capacity(l.len());
                for e in l.iter() {
                    inner.push(self.str_repr(e, true));
                }
                if let Value::Tuple(l) = v {
                    if l.len() == 1 {
                        return format!("({},)", inner[0]);
                    }
                }
                if let Value::Set(l) = v {
                    if l.is_empty() {
                        return "set()".to_string();
                    }
                    return format!("{{{}}}", inner.join(", "));
                }
                format!("[{}]", inner.join(", "))
            }
            Value::Dict(d) => {
                let mut inner: Vec<String> = Vec::with_capacity(d.len());
                for (k, val) in d.iter() {
                    inner.push(format!(
                        "{}: {}",
                        self.str_repr(&k.to_value(), true),
                        self.str_repr(val, true)
                    ));
                }
                format!("{{{}}}", inner.join(", "))
            }
            Value::Instance(i) => {
                let str_method = {
                    let class = i.borrow().class.clone();
                    self.find_method(&class, "str")
                };
                if let Some(method) = str_method {
                    if let Ok(v) = self.invoke(
                        &method,
                        Some(i.clone()),
                        Vec::new(),
                    ) {
                        if let Value::Str(s) = v {
                            return s.to_string();
                        }
                        return self.str_repr(&v, nested);
                    }
                }
                format!("<{} object>", i.borrow().class.name)
            }
            Value::Function(f) => format!("<fn {}>", f.name),
            Value::Builtin(b) => format!("<builtin {}>", b.name),
            Value::Class(c) => format!("<class {}>", c.name),
            Value::Enum(e) => format!("<enum {}>", e.name),
            Value::BoundMethod(b) => format!("<bound {}.{}>", b.class.name, b.func.name),
            Value::SuperProxy(_) => "<super>".to_string(),
        }
    }
}

pub fn format_float(f: f64) -> String {
    if f.is_nan() {
        "nan".to_string()
    } else if f.is_infinite() {
        if f > 0.0 {
            "inf".to_string()
        } else {
            "-inf".to_string()
        }
    } else if f == 0.0 {
        "0.0".to_string()
    } else if f.fract() == 0.0 && f.abs() < 1e16 {
        format!("{:.1}", f)
    } else {
        let s = format!("{}", f);
        if s.contains('e') {
            s
        } else {
            s
        }
    }
}

// ================= imports and modules =================

impl Vm {
    /// Names provided by the standard library rather than user files.
    fn stdlib_module(&self, name: &str) -> Option<Value> {
        self.globals.borrow().get(name).and_then(|v| match v {
            // Library modules are plain dict namespaces.
            Value::Dict(_) => Some(v.clone()),
            _ => None,
        })
    }

    /// Resolve a module name to a file path and load it.
    pub fn load_module(&mut self, module: &str) -> ZResult<Value> {
        if let Some(v) = self.loaded_modules.get(module) {
            return Ok(v.clone());
        }
        // Standard library modules take precedence over same-named files.
        if let Some(v) = self.stdlib_module(module) {
            self.loaded_modules.insert(module.to_string(), v.clone());
            return Ok(v);
        }
        // Dotted names are treated as package paths: `a.b` -> `a/b`.
        let rel = module.replace('.', "/");
        let candidates = [
            self.entry_dir.join(format!("{}.z", rel)),
            self.entry_dir.join(&rel).join("init.z"),
            self.base_dir.join(format!("{}.z", rel)),
            self.base_dir.join(&rel).join("init.z"),
        ];
        for path in candidates.iter() {
            if path.exists() {
                return self.load_file(module, path);
            }
        }
        Err(ZError::raised(
            "ImportError",
            format!("cannot find module '{}'", module),
        ))
    }

    /// Resolve a `dir (path) import ...` statement to a file or package.
    pub fn load_dir_module(&mut self, dir: &str) -> ZResult<Value> {
        let cleaned = dir.trim_matches(|c| c == '"' || c == '\'');
        let mut candidates = Vec::new();
        let direct = PathBuf::from(cleaned);
        candidates.push(direct.clone());
        candidates.push(self.entry_dir.join(&direct));
        candidates.push(self.base_dir.join(&direct));

        for base in candidates {
            if base.is_dir() {
                let init = base.join("init.z");
                if init.exists() {
                    return self.load_file_dir(&init);
                }
                continue;
            }
            let as_file = base.with_extension("z");
            if as_file.exists() {
                return self.load_file_dir(&as_file);
            }
            if base.exists() && base.extension().map(|e| e == "z").unwrap_or(false) {
                return self.load_file_dir(&base);
            }
        }
        Err(ZError::raised(
            "ImportError",
            format!("cannot find directory module '{}'", dir),
        ))
    }

    fn load_file(&mut self, key: &str, path: &std::path::Path) -> ZResult<Value> {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if self.loading.contains(&canonical) {
            return Err(ZError::raised(
                "ImportError",
                format!("circular import involving '{}'", key),
            ));
        }
        if let Some(v) = self.loaded_modules.get(key) {
            return Ok(v.clone());
        }
        let source = std::fs::read_to_string(path).map_err(|e| {
            ZError::raised(
                "ImportError",
                format!("cannot read '{}': {}", path.display(), e),
            )
        })?;

        // Module bodies run once in their own scope; public names form the namespace.
        if let Some(parent) = canonical.parent() {
            self.base_dir = parent.to_path_buf();
        }
        self.loading.push(canonical.clone());

        let module_scope = Rc::new(RefCell::new(Scope::new(self.globals.clone())));
        let result = (|| -> ZResult<()> {
            let module = crate::compiler::compile(&source, Some(&path.display().to_string()))?;
            let saved_scope = self.scope.clone();
            self.scope = module_scope.clone();
            let res = self.exec_block(&module.statements());
            self.scope = saved_scope;
            res.map(|_| ())
        })();

        self.loading.pop();
        result?;

        // Export public names under their own identifiers.
        let mut fields = Map::new();
        for (name, value) in module_scope.borrow().vars.iter() {
            if !name.starts_with('_') {
                fields.insert(ValueKey::Str(name.clone()), value.clone());
            }
        }
        let value = Value::dict(fields);
        self.loaded_modules.insert(key.to_string(), value.clone());
        Ok(value)
    }
    fn load_file_dir(&mut self, path: &std::path::Path) -> ZResult<Value> {
        let source = std::fs::read_to_string(path).map_err(|e| {
            ZError::raised(
                "ImportError",
                format!("cannot read '{}': {}", path.display(), e),
            )
        })?;
        if let Some(parent) = path.parent() {
            self.base_dir = parent.to_path_buf();
        }
        let module = crate::compiler::compile(&source, Some(&path.display().to_string()))?;
        let saved_scope = self.scope.clone();
        let module_scope = Rc::new(RefCell::new(Scope::new(self.globals.clone())));
        self.scope = module_scope.clone();
        let result = self.exec_block(&module.statements());
        self.scope = saved_scope;
        result?;

        let mut fields = Map::new();
        for (name, value) in module_scope.borrow().vars.iter() {
            if !name.starts_with('_') {
                fields.insert(ValueKey::Str(name.clone()), value.clone());
            }
        }
        Ok(Value::dict(fields))
    }
}

// ================= helpers used by built-ins =================

impl Vm {
    /// Mutate a container in place when possible, so `x.append(1)` is visible
    /// to the caller; otherwise the caller re-reads the container.
    /// Mutate `obj` to hold `updated`.
    ///
    /// When the container is uniquely owned it is rewritten in place, so every
    /// alias sees the change. Otherwise the owner (a variable, a scope or an
    /// object field) is rebound to the new container.
    pub fn replace_container(&mut self, obj: &Value, updated: Value) -> ZResult<Value> {
        // Values are immutable, so the owner is rebound to the new container.
        self.rebind_in_scopes(obj, updated)
    }

    fn rebind_in_scopes(&mut self, old: &Value, updated: Value) -> ZResult<Value> {
        // Search locals, enclosing scopes and instance fields for the owner.
        if self.scope.borrow().find_container(old) {
            let mut scope = self.scope.borrow_mut();
            scope.replace_container(old, updated.clone());
            return Ok(updated);
        }
        if let Some((inst, _)) = self.self_stack.last().cloned() {
            if find_in_instance(&inst, old) {
                replace_in_instance(&inst, old, &updated);
                return Ok(updated);
            }
        }
        // The container may belong to any live object, not just `self`.
        for inst in self.instances.clone() {
            if find_in_instance(&inst, old) {
                replace_in_instance(&inst, old, &updated);
                return Ok(updated);
            }
        }
        Ok(updated)
    }

    /// `isinstance(value, type_expr)`.
    pub fn isinstance(&mut self, value: &Value, type_expr: &Value) -> ZResult<Value> {
        Ok(Value::Bool(self.is_instance_of(value, type_expr)))
    }

    fn is_instance_of(&mut self, value: &Value, t: &Value) -> bool {
        match t {
            Value::Class(c) => match value {
                Value::Instance(i) => {
                    let mut cur = Some(i.borrow().class.clone());
                    while let Some(k) = cur {
                        if Rc::ptr_eq(&k, c) {
                            return true;
                        }
                        cur = self.parent_class(&k);
                    }
                    false
                }
                Value::Class(actual) => {
                    let mut cur = Some(actual.clone());
                    while let Some(k) = cur {
                        if Rc::ptr_eq(&k, c) {
                            return true;
                        }
                        cur = self.parent_class(&k);
                    }
                    false
                }
                _ => false,
            },
            Value::Str(name) => type_matches_name_value(value, name),
            Value::Builtin(b) => type_matches_name_value(value, &b.name),
            other => {
                let _ = other;
                false
            }
        }
    }

    /// `issubclass(cls, base)`.
    pub fn is_subclass(&mut self, sub: &Value, sup: &Value) -> ZResult<Value> {
        if let (Value::Class(s), Value::Class(u)) = (sub, sup) {
            let mut cur = Some(s.clone());
            while let Some(k) = cur {
                if k.name == u.name {
                    return Ok(Value::Bool(true));
                }
                cur = self.parent_class(&k);
            }
            return Ok(Value::Bool(false));
        }
        Ok(Value::Bool(false))
    }

    pub fn has_attr(&self, obj: &Value, name: &str) -> bool {
        self.attr_value_opt(obj, name).is_some()
            || crate::builtins::builtin_attr(obj, name).is_some()
    }

    /// Sort values into a new list, comparing by value semantics.
    pub fn sort_values(&mut self, items: Vec<Value>) -> ZResult<Vec<Value>> {
        let mut out = items;
        // Insertion sort: stable and lets us report a type error precisely.
        for i in 1..out.len() {
            let mut j = i;
            while j > 0 {
                let ord = self.compare_values_direct(&out[j - 1], &out[j])?;
                if ord == std::cmp::Ordering::Greater {
                    out.swap(j - 1, j);
                    j -= 1;
                } else {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Compare two values for ordering, used by `sorted`, `min` and `max`.
    pub fn compare_values_direct(
        &mut self,
        a: &Value,
        b: &Value,
    ) -> ZResult<std::cmp::Ordering> {
        self.compare(a, b)
    }
}

// ================= comprehensions =================

impl Vm {
    fn eval_list_comp(
        &mut self,
        elem: &Expr,
        target: &str,
        iter: &Expr,
        cond: Option<&Expr>,
    ) -> ZResult<Value> {
        let it = self.eval(iter)?;
        let items = self.iterate(&it)?;
        let names: Vec<String> = target
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let mut out = Vec::new();
        for item in items {
            let unpacked = unfold(item, names.len())?;
            self.push_comp_scope(&names, unpacked)?;
            let keep = match cond {
                Some(c) => self.eval(c)?.is_truthy(),
                None => true,
            };
            if keep {
                out.push(self.eval(elem)?);
            }
            self.pop_comp_scope();
        }
        Ok(Value::list(out))
    }

    fn eval_dict_comp(
        &mut self,
        key: &Expr,
        value: &Expr,
        target: &str,
        iter: &Expr,
        cond: Option<&Expr>,
    ) -> ZResult<Value> {
        let it = self.eval(iter)?;
        let items = self.iterate(&it)?;
        let names: Vec<String> = target
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let mut out = crate::value::Map::new();
        for item in items {
            let unpacked = unfold(item, names.len())?;
            self.push_comp_scope(&names, unpacked)?;
            let keep = match cond {
                Some(c) => self.eval(c)?.is_truthy(),
                None => true,
            };
            if keep {
                let k = self.eval(key)?;
                let v = self.eval(value)?;
                self.map_set(&mut out, k, v)?;
            }
            self.pop_comp_scope();
        }
        Ok(Value::dict(out))
    }

    /// Comprehensions run in a child scope so their loop variables do not leak.
    fn push_comp_scope(&mut self, names: &[String], values: Vec<Value>) -> ZResult<()> {
        let child = Rc::new(RefCell::new(Scope::child(self.scope.clone())));
        for (n, v) in names.iter().zip(values.into_iter()) {
            child.borrow_mut().vars.insert(n.clone(), v);
        }
        self.scope = child;
        Ok(())
    }

    fn pop_comp_scope(&mut self) {
        let parent = self.scope.borrow().parent.clone();
        if let Some(p) = parent {
            self.scope = p;
        }
    }
}

/// `isinstance(x, "int")` / `isinstance(x, int)` by type name.
fn type_matches_name_value(value: &Value, name: &str) -> bool {
    match name {
        "int" => matches!(value, Value::Int(_)),
        "float" => matches!(value, Value::Float(_)),
        "str" => matches!(value, Value::Str(_)),
        "bool" => matches!(value, Value::Bool(_)),
        "list" => matches!(value, Value::List(_)),
        "dict" => matches!(value, Value::Dict(_)),
        "tuple" => matches!(value, Value::Tuple(_)),
        "set" => matches!(value, Value::Set(_)),
        "fn" | "function" => matches!(value, Value::Function(_) | Value::Builtin(_)),
        "object" => matches!(value, Value::Instance(_)),
        "class" => matches!(value, Value::Class(_)),
        "None" | "NoneType" => matches!(value, Value::None),
        "Error" | "Exception" => matches!(value, Value::Instance(_)),
        _ => false,
    }
}

/// Build an `AttributeError` for a value and attribute name.
fn attr_error_of(v: &Value, attr: &str) -> ZError {
    ZError::raised(
        "AttributeError",
        format!(
            "'{}' object has no attribute '{}'",
            v.type_name(),
            attr
        ),
    )
}

/// True when an instance field (possibly nested) holds this exact container.
fn find_in_instance(inst: &InstanceRef, probe: &Value) -> bool {
    let inst = inst.borrow();
    inst.fields.values().any(|v| value_owns_container(v, probe))
        || inst.class.fields.values().any(|v| value_owns_container(v, probe))
}

/// Replace every reference inside an instance's fields with the new container.
fn replace_in_instance(inst: &InstanceRef, old: &Value, updated: &Value) {
    let mut inst = inst.borrow_mut();
    for field in inst.fields.values_mut() {
        replace_container_nested(field, old, updated);
    }
}

/// True when two values are the same shared container.
fn same_container(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::List(x), Value::List(y)) => Rc::ptr_eq(x, y),
        (Value::Set(x), Value::Set(y)) => Rc::ptr_eq(x, y),
        (Value::Dict(x), Value::Dict(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

fn value_owns_container(value: &Value, probe: &Value) -> bool {
    if same_container(value, probe) {
        return true;
    }
    // Also look one level into containers held by the value.
    match value {
        Value::List(l) | Value::Tuple(l) | Value::Set(l) => {
            l.iter().any(|v| value_owns_container(v, probe))
        }
        Value::Dict(d) => d.values().any(|v| value_owns_container(v, probe)),
        _ => false,
    }
}

fn replace_container_nested(value: &mut Value, old: &Value, updated: &Value) {
    if same_container(value, old) {
        *value = updated.clone();
        return;
    }
    // Only lists and dicts can be rewritten in place; tuples are shared.
    match value {
        Value::List(l) => {
            if let Some(cell) = Rc::get_mut(l) {
                for item in cell.iter_mut() {
                    replace_container_nested(item, old, updated);
                }
            }
        }
        Value::Dict(d) => {
            if let Some(cell) = Rc::get_mut(d) {
                let keys = cell.keys_vec();
                for key in keys {
                    if let Some(item) = cell.get_mut(&key) {
                        replace_container_nested(item, old, updated);
                    }
                }
            }
        }
        _ => {}
    }
}
