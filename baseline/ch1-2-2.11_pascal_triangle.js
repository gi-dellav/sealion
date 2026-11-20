function pascal_triangle(row, index) {
    return index > row
           ? false
           : index === 1 || index===row
             ? 1
             : pascal_triangle(row - 1, index - 1)
	       +
	       pascal_triangle(row - 1, index);
}

pascal_triangle(5, 4);
