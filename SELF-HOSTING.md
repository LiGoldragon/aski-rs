# Self-Hosting Plan

## Status
- Types designed: ParamKind, Param, MethodSig, ExprKind, Expr, MethodDef, TraitDecl, TraitImpl
- Parser skeleton: parseTraitDecl, parseTraitImpl, parseMethodDefs (skips bodies)
- Codegen skeleton: emitTraitDecls, emitTraitImpls (empty bodies)
- ExprStack on ParseState for building expression trees

## What's needed

### 1. Expression Parser (parser.aski)
Methods to add:
- `parseExpr(@Self) ParseState` — dispatches on token, pushes Expr to ExprStack
- `parseStmts(@Self) ParseState` — parse statement list until delimiter
- `parseAtom(@Self) ParseState` — literals, @refs, PascalName, groups, struct construction
- `parsePostfix(@Self) ParseState` — .field, .method(args) chains
- `parseBinOp(@Self) ParseState` — + operator
- `parseMethodBody(@Self) ParseState` — [ stmts ] or [| stmts |] or (| arms |)

Expression kinds to handle:
- `~@Var Type/new(expr)` → MutableNew { name, type, children }
- `~@Var.set([expr])` → MutableSet { name, children }
- `@Var Type/new(expr)` → SubTypeNew / SameTypeNew
- `^expr` → Return { children }
- `#expr.method([body])` → Yield { children: [MethodCall] }
- `@Name` → InstanceRef { name }
- `"text"` → StringLit { value }
- `123` → IntLit { value }
- `PascalName` → BareName { name }
- `@Self.Field` → Access { name, children: [InstanceRef] }
- `@Self.method(args)` → MethodCall { name, children: [base, args...] }
- `[expr1 expr2 ...]` → InlineEval { children }
- `expr + expr` → BinOp { value: "+", children: [left, right] }
- `Type(Field(val))` → StructConstruct { name, children: [StructField...] }
- `(| target (Pat) result |)` → Match { children: [target, arms...] }
- `StdOut expr` → StdOut { children }

### 2. Expression Emitter (codegen.aski)  
Methods to add:
- `emitExpr(:@Self @E Expr) String` — dispatches on E.Kind
- `emitStmts(:@Self @Stmts Vec{Expr} @Indent String) String` — emit statement list
- `emitMethodBody(:@Self @M MethodDef) String` — emit full method

Emission patterns:
- MutableNew → `let mut {name}: {type} = {emitExpr(child)};`
- MutableSet → `{name} = {emitExpr(child)};`
- Return → `{emitExpr(child)}`
- InstanceRef → `{snake(name)}`
- StringLit → `"{value}"` (empty → `String::new()`)
- IntLit → `{value}`
- BareName → qualify(name)
- Access → `{emitExpr(base)}.{snake(name)}`
- MethodCall → `{emitExpr(base)}.{snake(name)}({emitExpr(args)})`
- BinOp → `({emitExpr(left)} {op} &{emitExpr(right)})`
- InlineEval → `{ stmt; stmt; result }`
- StructConstruct → `Type { field: val, ... }`
- Match → `match {target} { Pat => result, ... }`
- Yield → `for {var} in {collection}.iter() { body }`
- StdOut → `println!("{}", {emitExpr(child)})`

### 3. Wire through (askic.rs)
- Pass TraitDecls/TraitImpls in the parser→codegen conversion
- Update ParseState init to include ExprStack, TraitDecls, TraitImpls
- Update CodeWorld conversion to map TraitDecl/TraitImpl between parser and codegen types

### 4. Test
- `askic source/codegen.aski` output matches bootstrap-compiled codegen_gen.rs
- `askic source/parser.aski` output matches bootstrap-compiled parser_gen.rs
- nix flake check passes
