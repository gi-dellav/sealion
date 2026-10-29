function stringify_list(xs) {
    return is_null(member(head(xs),
                   list("assign", "perform", "op", "label", "branch",
                        "go_to", "save", "restore", "reg", "constant")))
           ? "list(" + comma_separated(xs) + ")"
           : head(xs) + "(" + comma_separated(tail(xs)) + ")";
}
function comma_separated(elements) {
    return accumulate((s, acc) => stringify_instruction(s) +
                                  (acc === "" ? "" : ", " + acc),
                      "", elements);
}
function stringify_instruction(x) {
    return is_string(x) || is_number(x) || is_undefined(x) || is_null(x)
           ? stringify(x)
           : stringify_list(x);
}
function display_instructions(instructions) {
    return for_each(i => {display(stringify_instruction(i) + ","); },
                    instructions);
}
