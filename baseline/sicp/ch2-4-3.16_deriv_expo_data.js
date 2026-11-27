import { head, list, tail } from './sicp.js';
function make_exponentiation(base, exp) {
    return list("**", base, exp);
}
function base(operands) {
    return head(operands);
}
function exponent(operands) {
    return head(tail(operands));
}
