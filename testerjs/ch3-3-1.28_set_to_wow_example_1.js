const x = list("a", "b");
const z1 = pair(x, x);
z1;

// expected: [ [ 'a', [ 'b', null ] ], [ 'a', [ 'b', null ] ] ]
