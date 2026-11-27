function strConcat(rounds, pieces) {
    let acc = 0;
    for (let r = 0; r < rounds; r = r + 1) {
        let s = "";
        for (let i = 0; i < pieces; i = i + 1) {
            s = s + "abcdefgh";
        }
        acc = (acc + s.length + s.charCodeAt(r)) % 65536;
    }
    return acc;
}

let result = strConcat(200, 2000) % 256;
result;
