function set_to_wow(x) {
    set_head(head(x), "wow");
    return x;
}
const x = list("a", "b");
const z1 = pair(x, x);
set_to_wow(z1);

// expected: [ [ 'wow', [ 'b', null ] ], [ 'wow', [ 'b', null ] ] ]
