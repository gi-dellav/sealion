function pair(x, y) {
    function dispatch(m) {
        return m === "head"
               ? x
               : m === "tail"
               ? y
               : error(m, "undefined operation -- pair");
    }
    return dispatch;	      
}

function head(z) { return z("head"); }

function tail(z) { return z("tail"); }

const x = pair(1, 2);
head(x);

// expected: 1
