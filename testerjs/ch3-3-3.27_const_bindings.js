const base = 3;

function scaled(x) {
    const factor = 2;
    return x * factor;
}

function test(x, y) {
    return x === 0 ? 0 : y;
}

test(scaled(base) - 6, 1);
