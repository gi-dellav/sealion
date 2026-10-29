function make_mobile(left, right) {
    return list(left, right);
}
function make_branch(length, structure) {
    return list(length, structure);
}
function left_branch(m) {
    return head(m);
}
function right_branch(m) {
    return head(tail(m));
}
function branch_length(b) {
    return head(b);
}
function branch_structure(b) {
    return head(tail(b));
}
