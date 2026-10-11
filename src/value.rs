//! Runtime value types for the Z language VM.

use crate::ast::{Param, Stmt};
use crate::error::ZResult;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// An insertion-ordered map, matching Z's dict semantics.
///
/// `order` keeps the insertion sequence and `entries` gives O(1) lookup.
#[derive(Debug, Clone, Default)]
pub struct Map {
    entries: HashMap<ValueKey, Value>,
    order: Vec<ValueKey>,
}

impl Map {
    pub fn new() -> Self {
        Map {
            entries: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn insert(&mut self, key: ValueKey, value: Value) -> Option<Value> {
        let previous = self.entries.insert(key.clone(), value);
        if previous.is_none() {
            self.order.push(key);
        }
        previous
    }

    pub fn get(&self, key: &ValueKey) -> Option<&Value> {
        self.entries.get(key)
    }

    pub fn contains_key(&self, key: &ValueKey) -> bool {
        self.entries.contains_key(key)
    }

    pub fn remove(&mut self, key: &ValueKey) -> Option<Value> {
        let removed = self.entries.remove(key);
        if removed.is_some() {
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
        }
        removed
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = &ValueKey> {
        self.order.iter()
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.order.iter().filter_map(move |k| self.entries.get(k))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ValueKey, &Value)> {
        self.order.iter().filter_map(move |k| self.entries.get(k).map(|v| (k, v)))
    }

    pub fn get_mut(&mut self, key: &ValueKey) -> Option<&mut Value> {
        self.entries.get_mut(key)
    }

    pub fn keys_vec(&self) -> Vec<ValueKey> {
        self.order.clone()
    }
}

impl PartialEq for Map {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
            && self.order.len() == other.order.len()
            && self.order.iter().eq(other.order.iter())
    }
}

pub type RawMap = HashMap<ValueKey, Value>;

/// A hashable wrapper used for dict keys and set membership.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueKey {
    None,
    Bool(bool),
    Int(i64),
    Float(u64), // ordered bits
    Str(String),
    Tuple(Vec<ValueKey>),
}

impl ValueKey {
    /// Convert any value into a key.
    ///
    /// Dict keys in Z must be hashable, but modules need to expose functions
    /// and classes as members, so those values get a unique synthetic key
    /// that still displays as the original name.
    pub fn from_value(v: &Value) -> Option<ValueKey> {
        match v {
            Value::None => Some(ValueKey::None),
            Value::Bool(b) => Some(ValueKey::Bool(*b)),
            Value::Int(n) => Some(ValueKey::Int(*n)),
            Value::Float(f) => Some(ValueKey::Float(f.to_bits())),
            Value::Str(s) => Some(ValueKey::Str((**s).clone())),
            Value::Tuple(t) => {
                let mut items = Vec::new();
                for e in t.iter() {
                    items.push(Self::from_value(e)?);
                }
                Some(ValueKey::Tuple(items))
            }
            // Modules store members by name; use the displayed name as the key.
            Value::Function(f) => Some(ValueKey::Str(format!("fn:{}", f.name))),
            Value::Builtin(b) => Some(ValueKey::Str(format!("builtin:{}", b.name))),
            Value::Class(c) => Some(ValueKey::Str(format!("class:{}", c.name))),
            Value::Enum(e) => Some(ValueKey::Str(format!("enum:{}", e.name))),
            other => Some(ValueKey::Str(format!("obj:{}", other.type_name()))),
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            ValueKey::None => Value::None,
            ValueKey::Bool(b) => Value::Bool(*b),
            ValueKey::Int(n) => Value::Int(*n),
            ValueKey::Float(bits) => Value::Float(f64::from_bits(*bits)),
            ValueKey::Str(s) => Value::str(s.clone()),
            ValueKey::Tuple(t) => {
                Value::Tuple(Rc::new(t.iter().map(|k| k.to_value()).collect()))
            }
        }
    }
}

impl Eq for ValueKey {}

impl std::hash::Hash for ValueKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            ValueKey::None => 0u8.hash(state),
            ValueKey::Bool(b) => {
                1u8.hash(state);
                b.hash(state)
            }
            ValueKey::Int(n) => {
                2u8.hash(state);
                n.hash(state)
            }
            ValueKey::Float(b) => {
                3u8.hash(state);
                b.hash(state)
            }
            ValueKey::Str(s) => {
                4u8.hash(state);
                s.hash(state)
            }
            ValueKey::Tuple(t) => {
                5u8.hash(state);
                t.hash(state)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Rc<String>),
    List(Rc<Vec<Value>>),
    Tuple(Rc<Vec<Value>>),
    Dict(Rc<Map>),
    Set(Rc<Vec<Value>>),
    Function(Rc<Function>),
    Builtin(Rc<BuiltinFn>),
    Instance(InstanceRef),
    Class(Rc<ClassDef>),
    Enum(Rc<EnumDef>),
    BoundMethod(BoundMethod),
    /// Receiver created by `super` inside an instance method.
    SuperProxy(InstanceRef),
}

/// An instance method bound to a concrete receiver, produced by attribute
/// access (`obj.method`) and by `callclass obj.method()`.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundMethod {
    pub instance: InstanceRef,
    pub class: Rc<ClassDef>,
    pub func: Rc<Function>,
}

impl Value {
    /// View an instance value, if it is one.
    pub fn as_instance(&self) -> Option<&InstanceRef> {
        match self {
            Value::Instance(i) => Some(i),
            _ => None,
        }
    }
    pub fn str<S: Into<String>>(s: S) -> Value {
        Value::Str(Rc::new(s.into()))
    }

    pub fn list(v: Vec<Value>) -> Value {
        Value::List(Rc::new(v))
    }

    pub fn tuple(v: Vec<Value>) -> Value {
        Value::Tuple(Rc::new(v))
    }

    pub fn dict(m: Map) -> Value {
        Value::Dict(Rc::new(m))
    }

    pub fn set(v: Vec<Value>) -> Value {
        Value::Set(Rc::new(v))
    }

    pub fn as_key(&self) -> Option<ValueKey> {
        ValueKey::from_value(self)
    }
}

#[derive(Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub is_async: bool,
    pub is_static: bool,
    /// Enclosing scope, for closures.
    pub closure: Option<Rc<std::cell::RefCell<Scope>>>,
    pub return_type: Option<String>,
    /// True when the body contains a `yield` — calling it collects the results.
    pub is_generator: bool,
}

impl PartialEq for Function {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

impl std::fmt::Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Function")
            .field("name", &self.name)
            .field("params", &self.params)
            .field("is_async", &self.is_async)
            .field("is_static", &self.is_static)
            .field("is_generator", &self.is_generator)
            .finish()
    }
}

#[derive(Clone)]
pub struct BuiltinFn {
    pub name: String,
    pub arity: Option<usize>,
    /// Sub-namespace, e.g. `printer.error`.
    pub members: Rc<HashMap<String, Value>>,
    pub func: Rc<dyn Fn(&mut crate::vm::Vm, &[Value], &Map) -> ZResult<Value>>,
}

impl fmt::Debug for BuiltinFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<builtin {}>", self.name)
    }
}

impl PartialEq for BuiltinFn {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub class: Rc<ClassDef>,
    pub fields: HashMap<String, Value>,
}

pub type InstanceRef = Rc<std::cell::RefCell<Instance>>;

pub fn new_instance(class: Rc<ClassDef>) -> InstanceRef {
    Rc::new(std::cell::RefCell::new(Instance {
        class,
        fields: HashMap::new(),
    }))
}

impl Instance {
    /// Read `self.attr`, falling back to class-level fields.
    pub fn get_field(&self, name: &str) -> Option<Value> {
        self.fields
            .get(name)
            .or_else(|| self.class.fields.get(name))
            .cloned()
    }

    pub fn set_field(&mut self, name: &str, value: Value) {
        self.fields.insert(name.to_string(), value);
    }
}

/// Helpers on the shared instance reference used throughout the VM.
pub fn instance_get(rc: &InstanceRef, name: &str) -> Option<Value> {
    rc.borrow().get_field(name)
}

pub fn instance_set(rc: &InstanceRef, name: &str, value: Value) {
    rc.borrow_mut().set_field(name, value);
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDef {
    pub name: String,
    pub parent: Option<String>,
    pub methods: HashMap<String, Rc<Function>>,
    pub class_methods: HashMap<String, Rc<Function>>,
    pub fields: HashMap<String, Value>,
}

impl ClassDef {
    pub fn get_instance_method(&self, name: &str) -> Option<Rc<Function>> {
        self.methods.get(name).cloned()
    }

    pub fn get_class_method(&self, name: &str) -> Option<Rc<Function>> {
        self.class_methods.get(name).cloned()
    }

    /// Find a method anywhere in the class, preferring instance methods.
    pub fn any_method(&self, name: &str) -> Option<Rc<Function>> {
        self.methods
            .get(name)
            .or_else(|| self.class_methods.get(name))
            .cloned()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<(String, Value)>,
}

/// A lexical scope: local variables plus links to enclosing scopes and globals.
pub struct Scope {
    pub vars: HashMap<String, Value>,
    pub parent: Option<Rc<std::cell::RefCell<Scope>>>,
    pub globals: Rc<std::cell::RefCell<HashMap<String, Value>>>,
    pub global_names: Vec<String>,
    pub outer_names: Vec<String>,
}

impl Scope {
    pub fn new(globals: Rc<std::cell::RefCell<HashMap<String, Value>>>) -> Self {
        Scope {
            vars: HashMap::new(),
            parent: None,
            globals,
            global_names: Vec::new(),
            outer_names: Vec::new(),
        }
    }

    pub fn child(parent: Rc<std::cell::RefCell<Scope>>) -> Self {
        let globals = parent.borrow().globals.clone();
        Scope {
            vars: HashMap::new(),
            parent: Some(parent),
            globals,
            global_names: Vec::new(),
            outer_names: Vec::new(),
        }
    }

    pub fn is_global(&self, name: &str) -> bool {
        self.global_names.iter().any(|n| n == name)
    }

    pub fn is_outer(&self, name: &str) -> bool {
        self.outer_names.iter().any(|n| n == name)
    }

    fn lookup_skipping_outer(&self, name: &str) -> Option<Value> {
        if self.vars.contains_key(name) {
            return self.vars.get(name).cloned();
        }
        match &self.parent {
            Some(p) => p.borrow().lookup_skipping_outer(name),
            None => None,
        }
    }

    fn contains_local_or_outer(&self, name: &str) -> bool {
        if self.vars.contains_key(name) {
            return true;
        }
        match &self.parent {
            Some(p) => p.borrow().contains_local_or_outer(name),
            None => false,
        }
    }

    /// Resolve a name to a value, or `None` when it is unknown.
    pub fn lookup(&self, name: &str) -> Option<Value> {
        if self.is_outer(name) {
            if let Some(p) = &self.parent {
                return p.borrow().lookup_skipping_outer(name);
            }
        }
        if self.is_global(name) {
            return self.globals.borrow().get(name).cloned();
        }
        if let Some(v) = self.vars.get(name) {
            return Some(v.clone());
        }
        match &self.parent {
            Some(p) => p.borrow().lookup(name),
            None => self.globals.borrow().get(name).cloned(),
        }
    }

    /// Assign to a variable, following scope rules.
    pub fn assign(&mut self, name: &str, value: Value) {
        if self.is_outer(name) {
            if let Some(p) = &self.parent {
                p.borrow_mut().assign_existing(name, value);
                return;
            }
        }
        if self.is_global(name) {
            self.globals.borrow_mut().insert(name.to_string(), value);
            return;
        }
        if self.vars.contains_key(name) {
            self.vars.insert(name.to_string(), value);
            return;
        }
        if self.parent.is_some() && self.contains_local_or_outer(name) {
            if let Some(p) = &self.parent {
                p.borrow_mut().assign_existing(name, value);
            }
            return;
        }
        if self.parent.is_none() && self.globals.borrow().contains_key(name) {
            self.globals.borrow_mut().insert(name.to_string(), value);
            return;
        }
        self.vars.insert(name.to_string(), value);
    }

    pub fn assign_existing(&mut self, name: &str, value: Value) {
        if self.vars.contains_key(name) {
            self.vars.insert(name.to_string(), value);
            return;
        }
        if let Some(p) = &self.parent {
            p.borrow_mut().assign_existing(name, value);
        }
    }

    /// True when any reachable variable currently holds this exact container.
    pub fn find_container(&self, probe: &Value) -> bool {
        let holds = |v: &Value| match (probe, v) {
            (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
            (Value::Dict(a), Value::Dict(b)) => Rc::ptr_eq(a, b),
            (Value::Set(a), Value::Set(b)) => Rc::ptr_eq(a, b),
            _ => false,
        };
        if self.vars.values().any(holds) {
            return true;
        }
        // Also look inside nested containers, e.g. `abl rows = [mylist]`.
        if self.vars.values().any(|v| contains_container(v, probe)) {
            return true;
        }
        match &self.parent {
            Some(p) => p.borrow().find_container(probe),
            None => false,
        }
    }

    /// Replace every variable holding `old` with the updated container.
    pub fn replace_container(&mut self, old: &Value, updated: Value) {
        let holds = |v: &Value| match (old, v) {
            (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
            (Value::Dict(a), Value::Dict(b)) => Rc::ptr_eq(a, b),
            (Value::Set(a), Value::Set(b)) => Rc::ptr_eq(a, b),
            _ => false,
        };
        for v in self.vars.values_mut() {
            if holds(v) {
                *v = updated.clone();
            } else {
                replace_nested(v, old, &updated);
            }
        }
        if let Some(p) = &self.parent {
            p.borrow_mut().replace_container(old, updated);
        }
    }
}

fn contains_container(value: &Value, probe: &Value) -> bool {
    match (value, probe) {
        (Value::List(a), Value::List(b))
        | (Value::Tuple(a), Value::Tuple(b))
        | (Value::Set(a), Value::Set(b)) => {
            Rc::ptr_eq(a, b)
                || a.iter().any(|v| contains_container(v, probe))
        }
        (Value::Dict(a), Value::Dict(b)) => {
            Rc::ptr_eq(a, b)
                || a.values().any(|v| contains_container(v, probe))
                || a.keys().any(|k| {
                    contains_container(&k.to_value(), probe)
                })
        }
        _ => false,
    }
}

fn replace_nested(value: &mut Value, old: &Value, updated: &Value) {
    match value {
        Value::List(l) | Value::Tuple(l) | Value::Set(l) => {
            if let Some(cell) = Rc::get_mut(l) {
                for item in cell.iter_mut() {
                    if let Value::List(a) = &*item {
                        if let Value::List(b) = old {
                            if Rc::ptr_eq(a, b) {
                                *item = updated.clone();
                                continue;
                            }
                        }
                    } else if let Value::Dict(a) = &*item {
                        if let Value::Dict(b) = old {
                            if Rc::ptr_eq(a, b) {
                                *item = updated.clone();
                                continue;
                            }
                        }
                    } else if let Value::Set(a) = &*item {
                        if let Value::Set(b) = old {
                            if Rc::ptr_eq(a, b) {
                                *item = updated.clone();
                                continue;
                            }
                        }
                    }
                    replace_nested(item, old, updated);
                }
            }
        }
        Value::Dict(d) => {
            if let Some(cell) = Rc::get_mut(d) {
                let keys: Vec<ValueKey> = cell.keys().cloned().collect();
                for k in keys {
                    if let Some(v) = cell.get_mut(&k) {
                        if let Value::List(a) = v {
                            if let Value::List(b) = old {
                                if Rc::ptr_eq(a, b) {
                                    *v = updated.clone();
                                    continue;
                                }
                            }
                        }
                        replace_nested(v, old, updated);
                    }
                }
            }
        }
        _ => {}
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::None => write!(f, "None"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(fl) => {
                if fl.fract() == 0.0 && fl.is_finite() && fl.abs() < 1e16 {
                    write!(f, "{:.1}", fl)
                } else {
                    write!(f, "{}", fl)
                }
            }
            Value::Str(s) => write!(f, "{}", s),
            Value::List(l) => {
                let items: Vec<String> = l.iter().map(|v| repr(v)).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Value::Tuple(t) => {
                if t.len() == 1 {
                    return write!(f, "({},)", repr(&t[0]));
                }
                let items: Vec<String> = t.iter().map(|v| repr(v)).collect();
                write!(f, "({})", items.join(", "))
            }
            Value::Dict(d) => {
                let items: Vec<String> = d
                    .iter()
                    .map(|(k, v)| format!("{}: {}", repr(&k.to_value()), repr(v)))
                    .collect();
                write!(f, "{{{}}}", items.join(", "))
            }
            Value::Set(s) => {
                if s.is_empty() {
                    return write!(f, "set()");
                }
                let items: Vec<String> = s.iter().map(|v| repr(v)).collect();
                write!(f, "{{{}}}", items.join(", "))
            }
            Value::Function(g) => write!(f, "<fn {}>", g.name),
            Value::Builtin(b) => write!(f, "<builtin {}>", b.name),
            Value::Instance(i) => write!(f, "<{} object>", i.borrow().class.name),
            Value::Class(c) => write!(f, "<class {}>", c.name),
            Value::Enum(e) => write!(f, "<enum {}>", e.name),
            Value::BoundMethod(b) => write!(f, "<bound {}.{}>", b.class.name, b.func.name),
            Value::SuperProxy(_) => write!(f, "<super>"),
        }
    }
}

/// Rendering used inside collections: strings keep their quotes.
pub fn repr(v: &Value) -> String {
    match v {
        Value::Str(s) => format!("'{}'", s),
        Value::None => "None".to_string(),
        Value::Bool(b) => b.to_string(),
        _ => v.to_string(),
    }
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::None => "None",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "str",
            Value::List(_) => "list",
            Value::Tuple(_) => "tuple",
            Value::Dict(_) => "dict",
            Value::Set(_) => "set",
            Value::Function(_) => "fn",
            Value::Builtin(_) => "builtin",
            Value::Instance(_) => "object",
            Value::Class(_) => "class",
            Value::Enum(_) => "enum",
            Value::BoundMethod(_) => "bound method",
            Value::SuperProxy(_) => "super",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::None => false,
            Value::Int(n) => *n != 0,
            Value::Float(fl) => *fl != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Tuple(t) => !t.is_empty(),
            Value::Dict(d) => !d.is_empty(),
            Value::Set(s) => !s.is_empty(),
            _ => true,
        }
    }

    pub fn as_float(&self) -> f64 {
        match self {
            Value::Int(n) => *n as f64,
            Value::Float(fl) => *fl,
            Value::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }

    pub fn to_bool(&self) -> Value {
        Value::Bool(self.is_truthy())
    }
}
