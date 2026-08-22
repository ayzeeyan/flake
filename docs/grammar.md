# Flake grammar (v0.5 sketch)

```
program     := item*
item        := fn | struct | enum | type-alias | import
fn          := "pub"? "strict"? "owned"? "fn" ident "(" params? ")"
               ("->" type)? ("/" effects)? block
params      := param ("," param)* ","?
param       := ident (":" type)?
effects     := ident ("+" ident)*
struct      := "pub"? "struct" ident "{" (ident ":" type)* "}"
enum        := "pub"? "enum" ident "{" variant* "}"
variant     := ident ("(" type ("," type)* ")")?
type-alias  := "pub"? "type" ident "=" type
import      := "import" path ("as" ident)?
path        := ident ("." ident)*

block       := "{" stmt* expr? "}"
stmt        := let | var | return | break | continue
             | while | for | loop | expr
let         := "let" ident (":" type)? "=" expr
var         := "var" ident (":" type)? "=" expr

expr        := assignment
assignment  := or (("=" | "+=" | "-=" | "*=" | "/=" | "%=") assignment)?
or          := and ("||" and)*
and         := equality ("&&" equality)*
equality    := cmp (("==" | "!=") cmp)*
cmp         := range (("<" | "<=" | ">" | ">=") range)*
range       := term (".." term)?
term        := factor (("+" | "-") factor)*
factor      := unary (("*" | "/" | "%") unary)*
unary       := ("-" | "!" | "&" "mut"? | "await") unary
             | "spawn" postfix | postfix
postfix     := primary (call | index | field | "?")*
primary     := ident | literal | list | map | string | "if" | "match" | block | "(" expr ")"
match       := "match" expr "{" arm* "}"
arm         := pattern "=>" expr
pattern     := "_" | ident | literal
             | path "." ident ("(" ident ("," ident)* ")")?

type        := "owned" type | "ref" type | "mut" type | "&" "mut"? type
             | atom "?"?
atom        := "dyn" | path ("[" type ("," type)* "]")? | "[" type "]"
             | "fn" "(" types? ")" ("->" type)? ("/" effects)?
```

`spawn` requires a call expression (`spawn work(args...)`) and produces
`Task[T]`. `await` consumes a task handle and produces its result. Both carry
the `conc` effect.

Postfix `?` accepts a Result-like enum declared with exactly
`Ok(value)` followed by `Err(error)`. It unwraps `Ok` and returns `Err` from
the enclosing function.

Newlines are statement separators. Semicolons are optional. `//` and nested
`/* */` comments are skipped by the lexer.

Dotted import paths are rooted at the directory containing the entry file;
single-segment imports first check next to the importer. Only explicit `pub`
items are exported. See [modules.md](modules.md) for resolution, aliases,
visibility, and ambiguity rules.
