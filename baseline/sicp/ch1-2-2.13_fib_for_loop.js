function fib(n) {
    let previous = 0;
    let current = 1;
    for (let i = 0; i < n; i = i + 1) {
        let next = previous + current;
        previous = current;
        current = next;
    }
    return previous;
}

function test(x, y) {
    return x === 0 ? 0 : y;
}

test(fib(10) - 55, 1);
