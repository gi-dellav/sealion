const one = f => x => f(x);
const two = f => x => f(f(x));

function plus(n, m) {
    return f => x => n(f)(m(f)(x));
}

// testing

const three = plus(one, two);

function church_to_number(c) {
    return c(n => n + 1)(0);
}
church_to_number(three);

// expected: 3
