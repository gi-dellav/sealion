//recursive function
function f_recursive(n) {
    return n < 3
           ? n
           : f_recursive(n - 1) +
	     2 * f_recursive(n - 2) +
	     3 * f_recursive(n - 3);
}

f_recursive(5);

// expected: 25
