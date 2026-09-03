# Flake v1.0 Formal Grammar

This document provides the formal EBNF grammar for the frozen Flake v1.0 stable subset.

```ebnf
program     := item*
item        := fn | const-item | struct | enum | type-alias | import | trait | impl

fn          := "pub"? "strict"? "owned"? "const"? "fn" ident type-params? "(" params? ")"
               ("->" type)? ("/" effects)? block
const-item  := "pub"? "const" ident ":" type "=" expr
params      := param ("," param)* ","?
param       := ident (":" type)?
effects     := ident ("+" ident)*

struct      := "pub"? "struct" ident type-params? "{" (ident ":" type)* "}"
enum        := "pub"? "enum" ident type-params? "{" variant* "}"
variant     := ident ("(" type ("," type)* ")")?
type-alias  := "pub"? "type" ident type-params? "=" type
type-params := "[" type-param ("," type-param)* "]"
type-param  := ident (":" bound ("+" bound)*)?
bound       := ident

trait       := "pub"? "trait" ident type-params? "{" trait-method* "}"
trait-method:= "fn" ident type-params? "(" params? ")" ("->" type)? ("/" effects)?
impl        := "impl" type-params? ident "for" type "{" fn* "}"

import      := "pub"? "import" path ("as" ident)?
path        := ident ("." ident)*

block       := "{" stmt* expr? "}"
nursery     := "nursery" block
stmt        := let | var | return | break | continue
             | while | for | loop | expr
let         := "let" ident (":" type)? "=" expr
var         := "var" ident (":" type)? "=" expr
return      := "return" expr?
break       := "break"
continue    := "continue"
while       := "while" expr block
for         := "for" ident "in" expr block
loop        := "loop" block

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
call        := "(" args? ")"
args        := expr ("," expr)* ","?
index       := "[" expr "]"
field       := "." ident
primary     := ident | literal | list | map | string | "if" | "match" | nursery | block | "(" expr ")"
if          := "if" expr block ("else" (if | block))?
match       := "match" expr "{" arm* "}"
arm         := pattern "=>" expr
pattern     := "_" | ident | literal
             | (path ".")? ident ("(" pattern ("," pattern)* ")")?
             | "[" pattern ("," pattern)* "]"

type        := "owned" type | "ref" type | "mut" type | "&" "mut"? type
             | atom "?"?
atom        := "dyn" | path ("[" type ("," type)* "]")? | "[" type "]"
             | "Task" "[" type "]"
             | "fn" "(" types? ")" ("->" type)? ("/" effects)?
types       := type ("," type)*
```

---

## Semantic Notes

1. **Structured Tasks (`spawn` / `await`)**:
   - `spawn <call>` requires a function call expression and the `conc` effect. It produces a scope-bound `Task[T]`.
   - `await <task>` consumes the task handle and yields its result `T`. A task may only be awaited once.
2. **Lexical Nurseries**:
   - `nursery { ... }` ensures that all tasks spawned within the block complete before execution leaves the block scope.
3. **Result Propagation (`?`)**:
   - Postfix `?` unwraps `Result.Ok(value)` or returns `Result.Err(err)` early from the enclosing function.
4. **CTFE Lite**:
   - `const NAME: T = <expr>` and `const fn` are evaluated at compile time under a fuel budget (`CTFE_FUEL = 10_000`) and call depth limit (`MAX_CALL_DEPTH = 256`).
