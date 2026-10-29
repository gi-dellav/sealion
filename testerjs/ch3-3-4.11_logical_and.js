function logical_and(s1, s2) {
    return s1 === 1 && s2 === 1
           ? 1
           : s1 === 0 || s1 === 1
           ? s2 === 0 || s2 === 1
             ? 0
             : error(s2, "invalid signal")
           : error(s1, "invalid signal");
}
