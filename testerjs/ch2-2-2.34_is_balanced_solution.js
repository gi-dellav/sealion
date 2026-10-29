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
function is_weight(x){
    return is_number(x);
}
function total_weight(x) {
    return is_weight(x)
        ? x
        : total_weight(branch_structure(
                         left_branch(x))) +
          total_weight(branch_structure(
                         right_branch(x)));
}
function is_balanced(x) {
    return is_weight(x) ||
        ( is_balanced(branch_structure(
                                  left_branch(x))) &&
          is_balanced(branch_structure(
                                  right_branch(x))) &&
          total_weight(branch_structure(
                                  left_branch(x))) 
          * branch_length(left_branch(x))
          ===
          total_weight(branch_structure(
                                  right_branch(x))) 
          * branch_length(right_branch(x))
        );
}

const m = make_mobile(
              make_branch(20, 
                  make_mobile(make_branch(10, 2),
                              make_branch(4, 5))), 
              make_branch(28, 5));
is_balanced(m);

// expected: true
