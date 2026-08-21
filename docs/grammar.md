# Flake grammar (v0.1, sketch)

```
program     := item*
item        := fn | struct | type-alias | import
fn          := "pub"? "strict"? "owned"? "fn" ident "(" params? ")"
               ("->" type)? ("/" effects)? block
params      := param ("," param)* ","?
param       := ident (":" type)?
effects     := ident ("+" ident)*
struct      := "pub"? "struct" ident "{" (ident ":" type)* "}"
type-alias  := "pub"? "type" ident "=" type
import      := "import" ident ("as" ident)?   // `import math` → sibling math.flk

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
unary       := ("-" | "!" | "&" "mut"?) unary | postfix
postfix     := primary (call | index | field)*
primary     := ident | literal | list | map | string | "if" | block | "(" expr ")"

type        := "owned" type | "ref" type | "mut" type | "&" "mut"? type
             | atom "?"?
atom        := "dyn" | ident ("[" type ("," type)* "]")? | "[" type "]"
             | "fn" "(" types? ")" ("->" type)? ("/" effects)?
```

Newlines are statement separators. Semicolons are optional. `//` and nested
`/* */` comments are skipped by the lexer.
