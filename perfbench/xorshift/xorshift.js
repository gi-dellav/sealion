function xorshift(n) {
    let x = 2463534242;
    let acc = 0;
    for (let i = 0; i < n; i = i + 1) {
        x = (x ^ (x << 13)) >>> 0;
        x = (x ^ (x >>> 17)) >>> 0;
        x = (x ^ (x << 5)) >>> 0;
        acc = (acc + x) % 65536;
    }
    return acc;
}

let result = xorshift(1000000) % 256;
result;
