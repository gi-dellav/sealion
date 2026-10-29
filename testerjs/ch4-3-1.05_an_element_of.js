// chapter=3 variant=non-det 
function an_element_of(items) {
    require(! is_null(items));
    return amb(head(items), an_element_of(tail(items)));
}

const xs = list("apple", "banana", "cranberry");
an_element_of(xs);	
// Press "Run" for the first solution. Type
// retry
// in the REPL on the right, for more solutions

// expected: 'apple'
