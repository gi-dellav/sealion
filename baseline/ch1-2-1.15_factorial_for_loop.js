function factorial(n) {
    let product = 1;
    for (let counter = 1; counter <= n; counter = counter + 1) {
        product = counter * product;
    }
    return product;
}

function test(x, y) {
    return x === 0 ? 0 : y;
}

test(factorial(5) - 120, 1);
