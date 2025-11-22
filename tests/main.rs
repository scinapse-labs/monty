use monty::{Executor, Exit, Object};

/// Formats an Object for debug output, following heap references to show actual values
fn format_object_debug(obj: &Object, heap: &monty::Heap) -> String {
    use monty::HeapData;

    match obj {
        Object::Ref(id) => match heap.get(*id) {
            HeapData::Str(s) => format!("Str({s:?})"),
            HeapData::Bytes(b) => format!("Bytes({b:?})"),
            HeapData::List(items) => {
                let formatted_items: Vec<String> = items.iter().map(|item| format_object_debug(item, heap)).collect();
                format!("List([{}])", formatted_items.join(", "))
            }
            HeapData::Tuple(items) => {
                let formatted_items: Vec<String> = items.iter().map(|item| format_object_debug(item, heap)).collect();
                format!("Tuple(({}),)", formatted_items.join(", "))
            }
        },
        other => format!("{other:?}"),
    }
}

macro_rules! parse_error_tests {
    ($($name:ident: $code:literal, $expected:literal;)*) => {
        $(
            paste::item! {
                #[test]
                fn [< parse_error_ $name >]() {
                    match Executor::new($code, "test.py", &[]) {
                        Ok(v) => panic!("parse unexpected passed, output: {v:?}"),
                        Err(e) => assert_eq!(e.summary(), $expected),
                    }
                }
            }
        )*
    }
}

parse_error_tests! {
    complex: "1+2j", "TODO: complex constants";
}

macro_rules! execute_ok_tests {
    ($($name:ident: $code:literal, $expected:expr;)*) => {
        $(
            paste::item! {
                #[test]
                fn [< execute_ok_ $name >]() {
                    let ex = Executor::new($code, "test.py", &[]).unwrap();
                    let result = ex.run(vec![]);
                    let output = match result {
                        Ok(Exit::Return(value)) => {
                            let heap = ex.heap();
                            format_object_debug(&value, &heap)
                        }
                        otherwise => panic!("Unexpected exit: {:?}", otherwise),
                    };
                    let expected = $expected.trim_matches('\n');
                    assert_eq!(output, expected);
                }
            }
        )*
    }
}

execute_ok_tests! {
    add_ints: "1 + 1", "Int(2)";
    add_strs: "'a' + 'b'", r#"Str("ab")"#;
    // language=Python
    for_loop_str_append_assign_op: "
v = ''
for i in range(1000):
    if i % 13 == 0:
        v += 'x'
len(v)
", "Int(77)";
    // language=Python
    for_loop_str_append_assign: "
v = ''
for i in range(1000):
    if i % 13 == 0:
        v = v + 'x'
len(v)
", "Int(77)";
    // language=Python
    shared_list_append: "
a = [1]
b = a
b.append(2)
len(a)
", "Int(2)";
}

macro_rules! execute_raise_tests {
    ($($name:ident: $code:literal, $expected_exc:expr;)*) => {
        $(
            paste::item! {
                #[test]
                fn [< execute_raise_ $name >]() {
                    let ex = Executor::new($code, "test.py", &[]).unwrap();
                    let result = ex.run(vec![]);
                    let output = match result {
                        Ok(Exit::Raise(exc_raise)) => {
                            format!("{}", exc_raise.exc)
                        }
                        otherwise => panic!("Unexpected raise: {:?}", otherwise),
                    };
                    let expected = $expected_exc.trim_matches('\n');
                    assert_eq!(output, expected);
                }
            }
        )*
    }
}

execute_raise_tests! {
    error_instance_str: "raise ValueError('testing')", "ValueError('testing')";
    raise_number: "raise 1 + 2", "TypeError('exceptions must derive from BaseException')";
    error_type: "raise TypeError", "TypeError()";
    error_no_args: "raise TypeError()", "TypeError()";
    error_string_arg: "raise TypeError('hello')", "TypeError('hello')";
    error_string_arg_quotes: "raise TypeError(\"hello 'there'\")", "TypeError(\"hello 'there'\")";
    // error_two_args: "raise ValueError('x', 1 + 2)", "ValueError('x', 3)";
}
