function make_monitored(f) {
    let counter = 0; //initialized to 0
    function mf(cmd) {
        if (cmd === "how many calls") {
            return counter;
        } else if (cmd === "reset count") {
            counter = 0;
            return counter;
        } else {
            counter = counter + 1;
            return f(cmd);
        }
    }
    return mf;
}

const s = make_monitored(math_sqrt);
s(100);
s("how many calls");
s(5);
s("how many calls");

// expected: 2
