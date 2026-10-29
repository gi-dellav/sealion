function is_variable(x) { return is_string(x); }
function is_same_variable(v1, v2) {
    return is_variable(v1) && is_variable(v2) && v1 === v2;
}
function is_sum(x) {
    return is_pair(x) && head(x) === "+";
}
function make_sum(a1, a2) { return list("+", a1, a2); }

function make_product(m1, m2) { return list("*", m1, m2); }
function addend(s) { return head(tail(s)); }
function augend(s) { return head(tail(tail(s))); }
function is_product(x) {
    return is_pair(x) && head(x) === "*";
}
function multiplier(s) { return head(tail(s)); }
function multiplicand(s) { return head(tail(tail(s))); }
function number_equal(exp, num) {
    return is_number(exp) && exp === num;
}
function base(e) {
    return head(tail(e));
}
function exponent(e) {
    return head(tail(tail(e)));
}
function make_exp(base, exp) {
    return number_equal(exp, 0)
           ? 1
           : number_equal(exp, 1)
             ? base
             : list("**", base, exp);
}
function is_exp(x) {
    return is_pair(x) && head(x) === "**";
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
                 : is_exp(exp)
                   ? make_product(make_product(exponent(exp),
                                      make_exp(
                                          base(exp),
                                          exponent(exp) - 1)),
			          deriv(base(exp), variable))
                   : error(exp, "unknown expression type -- deriv");
}

head(tail(head(tail(deriv(list("**", "x", 4), "x")))));

// expected: 4
