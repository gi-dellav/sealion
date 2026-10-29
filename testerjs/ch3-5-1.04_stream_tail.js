// chapter=2 
function stream_tail(stream) {
    return tail(stream)();
}

head(stream_tail(pair(4, () => pair(5, () => null))));

// expected: 5
