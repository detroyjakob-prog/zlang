# Z Grammar (EBNF) — v0.2.0

```
program      = { statement } ;
block        = "{" { statement } "}" ;

statement    = var_decl | const_decl | assignment | fn_decl | class_decl | enum_decl
             | if_stmt | match_stmt | while_stmt | for_stmt | try_stmt | with_stmt
             | import_stmt | return_stmt | yield_stmt | callerror_stmt | assert_stmt
             | del_stmt | scope_stmt | "break" | "continue" | "pass" | expr ;

var_decl     = "abl" [ type ] target { "," target } "=" expr { "," expr } ;
const_decl   = "const" IDENT "=" expr ;
assignment   = target { "," target } assign_op expr { "," expr } ;
assign_op    = "=" | "+=" | "-=" | "*=" | "/=" | "//=" | "%=" | "**=" ;
target       = IDENT | expr "." IDENT | expr "[" expr "]" | "*" IDENT ;

fn_decl      = [ "async" ] [ "static" ] "fn" IDENT "(" [ params ] ")" [ "->" type ] block ;
params       = param { "," param } ;
param        = [ type ] IDENT [ "=" expr ] | "*" IDENT | "**" IDENT ;
lambda       = "fn" "(" [ params ] ")" "=>" expr ;

class_decl   = IDENT [ "(" IDENT { "," IDENT } ")" ] "{" { class_member } "}" ;
class_member = fn_decl | var_decl ;
enum_decl    = "enum" IDENT "{" { IDENT [ "=" expr ] } "}" ;

if_stmt      = "if" expr block { "elif" expr block } [ "else" block ] ;
match_stmt   = "match" expr "{" { "case" expr { "," expr } block } "}" ;
while_stmt   = "while" expr block ;
for_stmt     = "for" target { "," target } "in" expr block ;
try_stmt     = "try" block { "except" [ IDENT { "," IDENT } [ "as" IDENT ] ] block }
               [ "else" block ] [ "finally" block ] ;
with_stmt    = "with" expr [ "as" IDENT ] block ;

import_stmt  = "import" IDENT [ "as" IDENT ]
             | "from" IDENT "import" name_list
             | "dir" "(" PATH ")" "import" name_list ;
name_list    = IDENT [ "as" IDENT ] { "," IDENT [ "as" IDENT ] } ;

return_stmt  = "return" [ expr { "," expr } ] ;
yield_stmt   = "yield" expr ;
callerror_stmt = "callerror" [ call_expr ] ;
assert_stmt  = "assert" expr [ "," expr ] ;
del_stmt     = "del" target ;
scope_stmt   = ( "global" | "outer" ) IDENT { "," IDENT } ;

expr         = ternary ;
ternary      = or_expr [ "if" or_expr "else" ternary ] ;
or_expr      = and_expr { "or" and_expr } ;
and_expr     = not_expr { "and" not_expr } ;
not_expr     = "not" not_expr | comparison ;
comparison   = bit_or { ( "==" | "!=" | "<" | ">" | "<=" | ">=" | "in" | "not" "in"
                        | "is" | "is" "not" ) bit_or } ;
bit_or       = bit_xor { "|" bit_xor } ;
bit_xor      = bit_and { "^" bit_and } ;
bit_and      = shift { "&" shift } ;
shift        = sum { ( "<<" | ">>" ) sum } ;
sum          = term { ( "+" | "-" ) term } ;
term         = unary { ( "*" | "/" | "//" | "%" ) unary } ;
unary        = ( "-" | "+" | "~" ) unary | power ;
power        = postfix [ "**" unary ] ;
postfix      = primary { "." IDENT | "[" slice "]" } ;
primary      = literal | IDENT | "(" expr ")" | list | dict | comprehension
             | lambda | call_expr | callclass_expr | await_expr ;

call_expr      = "call" postfix "(" [ args ] ")" ;
callclass_expr = "callclass" postfix "(" [ args ] ")" ;
await_expr     = "await" ( call_expr | callclass_expr ) ;
args         = arg { "," arg } ;
arg          = [ IDENT "=" ] expr | "*" expr | "**" expr ;
slice        = expr | [ expr ] ":" [ expr ] [ ":" [ expr ] ] ;

list         = "[" [ expr { "," expr } ] "]" ;
dict         = "{" [ expr ":" expr { "," expr ":" expr } ] "}" ;
comprehension = "[" expr "for" target "in" expr [ "if" expr ] "]"
              | "{" expr ":" expr "for" target { "," target } "in" expr [ "if" expr ] "}" ;

literal      = INT | FLOAT | STRING | FSTRING | "True" | "False" | "None" ;
type         = "int" | "float" | "str" | "bool" | "list" | "dict" | "tuple" | "set" | IDENT ;
```

Lexical rules: `//` starts a line comment, `/* */` a block comment, newlines end statements unless inside `()`, `[]`, or after an operator.
