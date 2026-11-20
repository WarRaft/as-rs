# as-rs

A parser, linter and semantic highlighter for **AngelScript**, a
statically typed scripting language with C++-like syntax, designed for
embedding in applications. It's built like the other language crates, on
the shared [`parser-rs`](../parser-rs).

- [The AngelScript language](#the-angelscript-language): a syntax reference to build on
- [Parser status](#parser-status): what `as-rs` handles today
- [CLI](#cli) · [Tests](#tests)

How the parser is built, and the rules every parser here follows, is in
the [`parser-rs` README](../parser-rs/README.md#principles).

> This reference describes the language as `as-rs` models it. Check edge
> cases, especially the operator precedence table, against the official
> AngelScript manual. The host application decides which types and
> functions exist (`string`, `array<T>`, `dictionary`, `print`, ...); the
> language itself only has the primitives.

---

## The AngelScript language

### Lexical structure

| element     | syntax                                                                                     |
|-------------|--------------------------------------------------------------------------------------------|
| comments    | `// line`, `/* block */`                                                                   |
| statements  | end with `;`; blocks in `{ }`                                                              |
| names       | letters, digits, `_`; case-sensitive                                                       |
| integers    | `42`, `0xFF`, `0b1010`, `0o17`, `0d42` (`_` separators allowed in prefixed forms)          |
| reals       | `1.5` (double), `1.5f` (float), `.5`, `1e10`                                               |
| strings     | `"..."` and `'...'` with escapes; heredoc `"""..."""` (multi-line, no escapes); adjacent literals are concatenated |
| constants   | `true`, `false`, `null`                                                                    |

**Reserved keywords:** `and auto bool break case cast catch class const
continue default do double else enum false float for funcdef if import in
inout int int8 int16 int32 int64 interface is mixin namespace not null or
out private protected return switch true try typedef uint uint8 uint16
uint32 uint64 void while xor`.

**Contextual keywords** are ordinary names except where used as keywords:
`abstract explicit external final from function get override property
set shared super this`.

### Types

- **Primitives:** `void`, `bool`, `int8`…`int64` (`int` = `int32`),
  `uint8`…`uint64` (`uint` = `uint32`), `float`, `double`, and `auto` for
  inferred locals.
- **Objects:** script classes and host-registered types, possibly
  namespaced (`Game::Unit`) and templated (`array<int>`,
  `dictionary`).
- **Handles** `T@`: a reference-counted pointer that can be `null`.
  `@h = @obj` rebinds the handle, while `h = obj` assigns the *value*.
  `is` / `!is` compare identity. `const T@` is a handle to a read-only
  object; `T@ const` is a handle that can't be rebound.
- **References** on parameters: `&in` (input copy), `&out` (output), and
  `&inout` or plain `&` (true reference, for reference types).
- **Arrays:** `int[]`, the same as `array<int>` (an add-on).
- **`?`** is the "any type" in host-registered function signatures.
- **Conversions:** implicit between numeric primitives (with possible
  precision warnings). Explicit forms are `int(x)` / `float(x)` for
  values and `cast<T>(h)` for handles.

### Declarations

```angelscript
namespace Game {                                  // namespaces nest; refer with Game::Name
    enum Team { Red, Blue = 2, Neutral }          // enum values: Team::Red or just Red
    funcdef void Callback(int value);             // function-signature type, used as Callback@
    typedef float real;                           // primitive aliases only
    const int MAX = 12;                           // globals; `const` needs an initializer
    interface IDamageable { void takeDamage(float); }

    shared class Unit : IDamageable {             // one base class + any interfaces
        private float hp;                         // members are public unless private/protected
        Unit(float hp) { this.hp = hp; }          // constructor
        ~Unit() {}                                // destructor
        float health {                            // virtual property
            get const { return hp; }
            set { hp = value; }                   // implicit parameter `value`
        }
        void takeDamage(float amount) override { hp -= amount; }
        bool opEquals(const Unit@ other) const {  // operator overloading by opXxx methods
            return other !is null && other.hp == hp;
        }
    }
}
import void ext(int) from "other_module";         // function from another module
```

Further rules:

- `shared` entities are shared between modules.
- `external shared class Foo;` declares one that another module defines.
- `abstract` and `final` classes and `final` / `override` methods work as
  in C++/Java/C#. A `mixin class` is pasted into the classes that list it
  as a base.
- Functions can be overloaded, and parameters can have defaults:
  `void f(int a, int b = 2)`.
- `const` after a method's parameter list makes it a read-only method.

### Statements

| statement   | syntax                                                                               |
|-------------|--------------------------------------------------------------------------------------|
| variables   | `int a = 1, b;`, `Foo f(1, 2);` (constructor arguments), `auto x = 1.5f;`           |
| expression  | `f(); x += 1;`                                                                       |
| if          | `if (c) ... else ...`                                                                |
| loops       | `while (c) ...`, `do ... while (c);`, `for (init; cond; inc, inc) ...`              |
| switch      | `switch (v) { case 1: case 2: ...; break; default: ... }` (integer/enum values)      |
| jumps       | `break;` (loops and switch), `continue;` (loops), `return [v];`                      |
| exceptions  | `try { ... } catch { ... }`                                                          |

### Expressions

| precedence (low → high) | operators                                                  |
|-------------------------|------------------------------------------------------------|
| 1                       | assignment `= += -= *= /= %= **= &= \|= ^= <<= >>= >>>=` (right) |
| 2                       | ternary `?:` (right)                                       |
| 3                       | `\|\|` `or`                                                |
| 4                       | `&&` `and`                                                 |
| 5                       | `==` `!=` `is` `!is` `^^` `xor`                            |
| 6                       | `<` `>` `<=` `>=`                                          |
| 7                       | `\|`                                                       |
| 8                       | `^`                                                        |
| 9                       | `&`                                                        |
| 10                      | `<<` `>>` `>>>`                                            |
| 11                      | `+` `-`                                                    |
| 12                      | `*` `/` `%`                                                |
| 13                      | `**` (right)                                               |
| 14                      | unary `-` `+` `!` `not` `~` `++` `--` `@`                  |
| 15                      | postfix `.m` `[i]` `(args)` `++` `--`                      |

- **Bitwise operators bind tighter than comparisons**, unlike C, so
  `a == b & c` is `a == (b & c)`. `xor` / `^^` is logical xor, at the
  equality level.
- **Operands:** literals, possibly namespaced names (`Game::MAX`), `this`,
  `super(...)`, `cast<T>(e)`, constructions `Type(args)` /
  `array<int>(3)`, init lists `{1, {2, 3}}` (in initializers), and
  lambdas `function(a, b) { ... }` (parameter types optional, when a
  funcdef is expected).

---

## Parser status

Supported: everything in the reference above. Not yet:

- metadata/attributes in `[...]` and preprocessor-style host directives
  (`#include`);
- named arguments (`f(b: 2)`) and `using namespace`;
- `opCall`-style ambiguities are read the C++ way: `T name(args)` is a
  declaration.

The declaration-or-expression question (`Foo f;` vs. `a < b;`) is
decided by trying to read a type followed by a name and rewinding if that
fails. `>>`/`>>>` close nested template lists (`array<array<int>>`).

Every lint diagnostic names its rule. The rule ids, which rules are on
by default (`infinite-loop` isn't) and how a project switches them
(`[lint]` in `wc3.bni`) are in the [`wc3-rs` README](../wc3-rs/README.md#lint).

Lint rules:

- **Errors:**
  - duplicate global declarations (functions may be overloaded), class
    members and enum values;
  - a local redeclared in the same scope;
  - `break` outside a loop or switch, `continue` outside a loop;
  - a non-`void` function or `get` accessor that doesn't return on every
    path (an endless loop counts as not returning).
- **Warnings:**
  - unused locals (parameters and `_`-prefixed names excluded);
  - a local hiding an outer local or parameter;
  - an endless `while (true)` / `for (;;)` with no `break` or `return`;
  - empty `if` bodies.

Lambdas and property accessors are checked as functions; `set` has its
implicit `value`.

Semantic highlighting:

- **Declarations and type names:** namespaces, classes, interfaces,
  enums and enum values, funcdefs and typedefs (`type`), functions,
  methods and constructors, fields, virtual properties, parameters,
  locals, globals, and constants (`readonly`).
- **Contextual keywords:** `this`, `super`, `get`, `set`, `override`,
  `final`, `shared`, `from` and lambda `function` are highlighted as
  keywords where they act as keywords.
- **Assignments** get the `modification` modifier.

It runs on the tolerant token stream: a type in declaration position
marks its names as types, and the name after it as a declaration.

**Symbols** (`symbols`, `semantic_tokens_and_symbols`): every name as a
`parser_rs::Occurrence`, recorded by the highlighting classifier in the
same pass, so they always agree.

| names | scope |
|-------|-------|
| namespaces, classes, interfaces, enums and their values, funcdefs, typedefs, global functions and variables | project, **in their namespace**: each records the namespace it's declared in or used from, and the qualifier written before it (`A::B` in `A::B::C`, `::` for the global namespace); the editor resolves a use from its namespace outwards (`Occurrence::resolve`). An enum's name in a qualifier is dropped: `Color::Red` and `Red` are the value in the enum's namespace |
| class members: fields, methods, constructors, virtual properties; any name after `.` | `Member`: matched by name, since `obj.x` can't be tied to a class without type inference |
| a bare name inside a class that names one of its members | that member (implicit `this`), resolved once the whole file is read |
| parameters and locals (methods, lambdas and `get`/`set` accessors too, `set`'s implicit `value` included) | their function (`Local(n)`) |

## CLI

```sh
cargo run -- file.as            # lint
cargo run -- --ast file.as      # indented AST tree
cargo run -- --dot file.as      # Graphviz DOT
cargo run -- --html file.as > file.ast.html && open file.ast.html
```

## Tests

```sh
cargo test
UPDATE_SNAPSHOTS=1 cargo test --test html_snapshot   # regenerate HTML goldens
```
