function stream_append(s1, s2) {
    return is_null(s1)
           ? s2
           : pair(head(s1), 
                  () => stream_append(stream_tail(s1), s2));
}

const ones = pair(1, () => ones);
const twos = pair(2, () => twos);
const appended = stream_append(ones, twos);
stream_ref(appended, 100);

// expected: 1
