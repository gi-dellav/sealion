// chapter=3 variant=non-det 
function an_integer_starting_from(n) {
    return amb(n, an_integer_starting_from(n + 1));
}

const x = an_integer_starting_from(1);
require(x >= 4.5);
x;
// Press "Run" for the first solution. Type
// retry
// in the REPL on the right, for more solutions

// expected: 5
