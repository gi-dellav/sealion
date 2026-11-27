function objProps(n) {
    let o = { a: 0, b: 0, c: 0, d: 0 };
    let keys = ["a", "b", "c", "d"];
    let acc = 0;
    for (let i = 0; i < n; i = i + 1) {
        o.a = o.a + 1;
        o.b = o.b + 2;
        o.c = o.c + 3;
        let k = keys[i & 3];
        o[k] = o[k] + 4;
        acc = (acc + o[k] + o.c) % 65536;
    }
    return acc;
}

let result = objProps(300000) % 256;
result;
