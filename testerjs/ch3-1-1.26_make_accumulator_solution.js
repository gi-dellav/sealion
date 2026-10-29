function make_accumulator(current) {
    function add(arg) {
        current = current + arg;
        return current;
    }
    return add;
}

const a = make_accumulator(5);
a(10);
a(10);

// expected: 25
