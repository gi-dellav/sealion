function is_variable(x) { return is_string(x); }
function is_same_variable(v1, v2) {
    return is_variable(v1) && is_variable(v2) && v1 === v2;
}
function number_equal(exp, num) {
    return is_number(exp) && exp === num;
}
function items_before_first(op, s) {
    return is_string(head(s)) && head(s) === op
           ? null
           : pair(head(s),
                  items_before_first(op, tail(s)));
}
function items_after_first(op, s) {
    return is_string(head(s)) && head(s) === op
           ? tail(s)
           : items_after_first(op, tail(s));
}
function simplify_unary_list(s) {
    return is_pair(s) && is_null(tail(s))
           ? head(s)
           : s;
}
function contains_plus(s) {
    return is_null(s)
           ? false
           : is_string(head(s)) && head(s) === "+"
             ? true
             : contains_plus(tail(s));
}
function make_sum(a1, a2) {
    return number_equal(a1, 0)
           ? a2
           : number_equal(a2, 0)
             ?  a1
             : is_number(a1) && is_number(a2)
               ? a1 + a2
               : list(a1, "+", a2);
}
// a sequence of terms and operators is a sum
// if and only if at least one + operator occurs
function is_sum(x) {
    return is_pair(x) && contains_plus(x);
}
function addend(s) {
    return simplify_unary_list(items_before_first("+", s));
}
function augend(s) {
    return simplify_unary_list(items_after_first("+", s));
}
function make_product(m1, m2) {
    return number_equal(m1, 0) || number_equal(m2, 0)
           ?  0
           : number_equal(m1, 1)
             ? m2
             : number_equal(m2, 1)
               ?  m1
               : is_number(m1) && is_number(m2)
                 ? m1 * m2
                 : list(m1, "*", m2);
}
// a sequence of terms and operators is a product
// if and only if no + operator occurs
function is_product(x) {
    return is_pair(x) && ! contains_plus(x);
}
function multiplier(s) {
    return simplify_unary_list(items_before_first("*", s));
}
function multiplicand(s) {
    return simplify_unary_list(items_after_first("*", s));
}
function deriv(exp, variable) {
    return is_number(exp)
        ? 0
        : is_variable(exp)
          ? (is_same_variable(exp, variable) ? 1 : 0)
          : is_sum(exp)
            ? make_sum(deriv(addend(exp), variable),
                       deriv(augend(exp), variable))
            : is_product(exp)
              ? make_sum(make_product(multiplier(exp),
                             deriv(multiplicand(exp),
                                   variable)),
                         make_product(deriv(multiplier(exp),
                                            variable),
                             multiplicand(exp)))
              : error(exp, "unknown expression type -- deriv");
}

deriv(list("x", "*", 4), "x");
