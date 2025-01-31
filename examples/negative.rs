use bpaf::*;

fn main() {
    let age = long("age").argument("AGE").from_str::<i64>();
    let msg = "\
To pass a value that starts with a dash requres one one of two special syntaxes:

This will pass '-1' to '--age' handler and leave remaining arguments as is
    --age=-1
This will transform everything after '--' into non flags, '--age' will handle '-1'
and positional handlers will be able to handle the rest.
    --age -- -1";
    let num = Info::default().descr(msg).for_parser(age).run();
    println!("age: {num}");
}
