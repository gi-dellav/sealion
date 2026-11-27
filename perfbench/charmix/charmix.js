function charMix(passes, n) {
    let src = "";
    for (let i = 0; i < n; i = i + 1) {
        src = src + String.fromCharCode(65 + (i % 26));
    }
    let acc = 0;
    for (let p = 0; p < passes; p = p + 1) {
        for (let i = 0; i < n; i = i + 1) {
            acc = (acc + src.charCodeAt(i) * ((i % 7) + 1)) % 65536;
        }
    }
    return acc;
}

let result = charMix(1000, 1024) % 256;
result;
