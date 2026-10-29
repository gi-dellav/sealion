// solution provided by GitHub user LucasGdosR

// The Fibonacci function receives n as an argument
// It applies the fib function recursively, passing n as an argument,
// as well as the initial arguments (k = 1, fib1 = 1, fib2 = 1)
(n => (fib => fib(fib, n, 2, 1, 1))
      // The fib function is then defined as ft,
      // with parameters n, k, fib1, and fib2
      // Establish the base cases: n === 1 or n === 2
      ((ft, n, k, fib1, fib2) => n === 1
                  ? 1
                  : n === 2
                  ? 1
                  :
                  // Iterate until k equals n. Notice k starts at 2, and gets incremented every iteration
                  k === n
                  // When k reaches n, return the accumulated fib2
                  ? fib2
                  // Otherwise, accumulate the sum as the new fib2
                  : ft(ft, n, k + 1, fib2, fib1 + fib2)));
