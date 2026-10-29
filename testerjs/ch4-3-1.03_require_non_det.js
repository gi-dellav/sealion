// chapter=3 variant=non-det 
function require(p) {
    if (! p) {
        amb();
    } else {}
}

const x = amb(1, 3, 5, 7, 9);
require(x >= 4);
x;
// Press "Run" for the first solution. Type
// retry
// in the REPL on the right, for more solutions

// expected: 5
