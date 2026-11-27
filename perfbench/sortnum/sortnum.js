function fill(a, n) {
    let x = 2463534242;
    for (let i = 0; i < n; i = i + 1) {
        x = (x ^ (x << 13)) >>> 0;
        x = (x ^ (x >>> 17)) >>> 0;
        x = (x ^ (x << 5)) >>> 0;
        a.push((x >>> 8) % 1000);
    }
    return a;
}

function fold(a, n) {
    let acc = 0;
    for (let i = 0; i < n; i = i + 1) {
        acc = (acc + (a[i] % 256) * ((i % 7) + 1)) % 65536;
    }
    return acc;
}

let n = 200000;
let a = fill([], n);
a.sort(function (x, y) {
    return x - y;
});

let result = fold(a, n) % 256;
result;
