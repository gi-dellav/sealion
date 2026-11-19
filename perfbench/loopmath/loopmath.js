function loopMath(n) {
    let acc = 0.0;
    for (let i = 1; i <= n; i = i + 1) {
        acc = acc + Math.sqrt(i) + (i * 0.5) / (i + 1.0);
    }
    return acc;
}

let result = Math.floor(loopMath(10000000)) % 256;
result;
