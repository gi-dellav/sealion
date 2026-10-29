function make_sum(a1, a2) {
    return list("+", a1, a2);
}
            
function make_product(m1, m2) {
    return list("*", m1, m2);
}
            
function addend(operands) {
    return head(operands);
}
            
function augend(operands) {
    return head(tail(operands));
}
            
function multiplier(operands) {
    return head(operands);
}
            
function multiplicand(operands) {
    return head(tail(operands));
}
