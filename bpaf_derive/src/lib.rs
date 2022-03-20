use quote::ToTokens;
use syn::parse_macro_input;

mod field;
mod kw;
mod top;
mod utils;
use top::*;

/// ## bpaf boilerplate deriving macro
///
/// `Bpaf` derive macro uses `bpaf` attribute and produces functions, not trait implementations. By
/// derive macro would combine struct fields and enum constructor fields using `construct!` macro,
/// enum constructors using `Parser::or_else` combinators. Named fields will generate named
/// arguments or flags depending on a value, unnamed fields will generate positional arguments.
/// Library tries to do a smart thing when picking names or how exactly things are parsed but it is
/// possible to change most of the aspects using annotations.
///
/// Annotations go into 4 different places: `ANN1`, `ANN2`, `ANN3` and `ANN4`.
/// ```ignore
/// #[derive(Bpaf)]
/// #[bpaf(ANN1)]
/// struct Foo {
///     #[bpaf(ANN3)]
///     field: usize
/// }
///
/// #[derive(Bpaf)]
/// #[bpaf(ANN1)]
/// enum Foo {
///     #[bpaf(ANN2)]
///     Bar {
///         #[bpaf(ANN3)]
///         field: usize
///     }
///     #[bpaf(ANN4)]
///     Baz,
/// }
/// ```
///
/// ## `struct`/`enum` annotations: `ANN1`
///
/// ### generate
/// By default bpaf would generate a function with a name derived from type name, `generate` allows to change this:
/// ```ignore
/// #[derive(Bpaf)]
/// struct MyStruct {
///   field: usize
/// }
///
/// #[derive(Bpaf)]
/// #[bpaf(generate("opts"))]
/// struct MyOtherStruct { field: usize }
/// ```
/// generates
/// ```ignore
/// fn my_struct() -> Parser<MyStruct> { /* ... */ }
///
/// fn opts() -> Parser<MyOtherStruct> { /* ... */ }
/// ```
///
/// ### options / command
/// By default bpaf would generate a regular parser with `construct!` operation for `struct`
/// and will combine branches from `enum` with `Parser::or_else`.
///
/// `options` will will decorate the parser with `Info` to produce
/// `OptionParser`.
///
/// `command` will further wrap those decorated options with a `command` wrapper
///
/// ```ignore
/// #[derive(Bpaf)]
/// struct Foo { field: usize }
///
/// #[derive(Bpaf)]
/// #[bpaf(options)]
/// struct Bar { field: usize }
///
/// #[derive(Bpaf)]
/// #[bpaf(command("cmd"))]
/// struct Baz { field: usize }
/// ```
///
/// ```ignore
/// fn foo() -> Parser<Foo> { /* ... */ }
///
/// fn bar() -> OptionParser<Bar> { Info::default().for_parser(/* ... */) }
///
/// fn baz() -> Parser<Bar> { command("foo", None, Info::default().for_parser(/* ... */)) }
/// ```
///
/// Description and help for options and command are derived from doc comments:
///
/// - first block up to double empty line goes into `descr` and `command` help
/// - next block up to the next double empty lines goes into `header`
/// - next block up to the next double empty lines goes into `footer`
///
/// ```ignore
/// #[derive(Bpaf)]
/// #[bpaf(options)]
/// /// short help
/// ///
/// ///
/// /// goes into header
/// ///
/// ///
/// /// goes into footer
/// struct Foo { field: usize }
/// ```
///
/// generates
/// ```ignore
/// fn foo() -> OptionParser<Foo> {
///     Info::default()
///         .descr("short help")
///         .header("goes into header")
///         .footer("goes into footer")
///         .for_parser(...)
/// }
/// ```
///
/// ## `enum` constructor annotations: `ANN2`
///
/// By default `bpaf` would generate regular construct parser, it is possible to override this
/// behavior with `command` attrubute that behaves similar to `ANN1`.
///
/// ## Field annotation: `ANN3`
///
/// Similar to field parser declarations using combinator Rust API field annotation
/// consists of roughly 3 parts that can be optional:
///
/// ((<naming> <consumer>) | <external>)  <postprocessing>
///
/// Bpaf tries to fill in missing parts when it can. API used tries to mimic usual Rust API, order
/// is important and user needs to ensure that generated code typechecks.
///
/// ## Naming
/// By default bpaf_derive tries to do a somewhat smart thing about the name:
/// if field name has more than one character it becomes a "long" name, otherwise
/// it becomes a "short" name. Automatic naming stops working if user specifies a
/// naming hint manually
///
/// User can opt to specify a name by using one or more of given hints:
/// - `short` - use first character as a short name
/// - `long` - use full field name as a long name, even if it's one symbol
/// - `short(lit)` - use a specific character as a short name
/// - `long(lit)` - using specific string as a long name
///
/// ```ignore
/// // no annotation
/// distance: usize
///
/// #[bpaf(long, short)]
/// speed: usize
///
/// #[bpaf(short('V'))]
/// velocity: usize
/// ```
///
/// generates
/// ```ignore
/// let distance = long("distance").argument("ARG").from_str::<usize>();
///
/// let speed = long("speed").short('s').argument("ARG").from_str::<usize>();
///
/// let let velocity = short('V').argument("ARG").from_str::<usize>();
///
/// ```
///
/// ## Annotations for fieldless enum constructors: `ANN4`
///
/// enum constructors without fields are transformed into required flags forcing user to specify
/// one of the options for parser to succeed. Only naming field annotations are accepted here.
///
/// ```ignore
/// #[derive(bpaf)]
/// enum Potato {
///     YesPlease,
///     No
/// }
/// ```
/// generates a parser that requires either `--yes-please` or --no` to be specified on a command
/// line:
/// ```ignore
/// {
///    let yes_please = long("yes-please").req_flag(Potato::YesPlease);
///    let no = long("no").req_flag(Potato::No);
///    construct!([yes_please, no])
/// }
/// ```
///
///
/// ## Help
///
/// Help message is generated from a doc comment, if one is present.
/// - bpaf skips single empty lines
/// - bpaf stops processing after double empty line
///
/// ```ignore
/// /// this is a help message
/// ///
/// /// so is this
/// ///
/// ///
/// /// but not this
/// field: usize
/// ```
///
/// generates
/// ```ignore
/// let field = long("field").help("this is a help message\nso is this").argument("ARG").from_str::<usize>();
/// ```
///
/// ## Consumer
///
/// By default bpaf tries to figure out which consumer to use type based on a field name and
/// resulting type: nameless fields are converted into positional arguments, named - into flag
/// arguments. `String` vs `OsString` is selected automatically if there's no user specifying parsing
/// transformations further down the line: `PathBuf` is parsed from `OsString`, everything else is
/// parsed from `String`.
///
/// ```ignore
/// #[derive(Bpaf)]
/// struct Foo(PathBuf);
/// #[derive(Bpaf)]
/// struct Bar { number: usize };
/// ```
/// generates
/// ```ignore
/// ...
/// let inner = positional_os("ARG").map(PathBuf::from); construct!(Foo(inner))
/// ...
/// let number = long("number").argument("ARG").from_str::<usize>(); construct!(Bar { number })
/// ```
///
/// When any "changing" postprocessing is present (`parse`, `map`, `from_str`, `many`, etc) - consumer needs top
/// be specified explicitly. In any case there can be only consumer.
///
/// ```ignore
/// fn parse_human_number(input: &str) -> Result<usize> { /* ... */ }
///
/// #[derive(Bpaf)]
/// struct Foo {
///     #[bpaf(argument("NUM"), parse(parse_human_number))]
///     number: usize
/// };
/// ```
///
/// ## Postprocessing
/// Operations from a list in first in first out order. anything other than `guard` and `fallback`
/// requres explicit consumer, otherwise the only requirement is to typecheck. Most postprocessing
/// components behave similar to their Rust API counterparts
/// - `guard` - takes a function name and a string literal
/// - `map` - takes a function name
/// - `parse` - takes a function name
/// - `parse_str` - takes a type
/// - `many` - takes no parameters
/// - `some` - takes no parameters
/// - `option` - takes no parameters
/// - `fallback` - takes an arbitrary expression

#[proc_macro_derive(Bpaf, attributes(bpaf))]
pub fn derive_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    parse_macro_input!(input as Top).to_token_stream().into()
}
