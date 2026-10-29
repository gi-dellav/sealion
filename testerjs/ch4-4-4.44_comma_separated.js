function comma_separated(strings) {
   return accumulate((s, acc) => s + (acc === "" ? "" : ", " + acc),
                     "",
                     strings);
}
