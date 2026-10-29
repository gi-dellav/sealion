// chapter=3 variant=non-det 
function a_pythogorean_triple_between(low, high) {      
    const i = an_integer_between(low, high);
    const j = an_integer_between(i, high);
    const k = an_integer_between(j, high);
    require(i * i + j * j === k * k);
    return list(i, j, k);
}

a_pythogorean_triple_between(5, 15);
// Press "Run" for the first solution. Type
// retry
// in the REPL on the right, for more solutions

// expected: [ 5, [ 12, [ 13, null] ] ]
