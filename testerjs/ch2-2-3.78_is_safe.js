function is_safe(k, positions) {
    const first_row = head(head(positions));
    const first_col = tail(head(positions));
    return accumulate((pos, so_far) => {
                         const row = head(pos);
                         const col = tail(pos);
                         return so_far &&
                                first_row - first_col !==
                                row - col &&
                                first_row + first_col !==
                                row + col &&
                                first_row !== row;
                      },
                      true, 
                      tail(positions));
}
