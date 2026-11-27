struct LazySequence<T> {
  var make: () -> () -> T?

  func map<U>(_ f: (T) -> U) -> LazySequence<U> {
    let outer = self.make
    return LazySequence<U>(make: {
      let next = outer()
      return { () -> U? in
        if let x = next() { return f(x) }
        return nil
      }
    })
  }

  func filter(_ p: (T) -> Bool) -> LazySequence<T> {
    let outer = self.make
    return LazySequence<T>(make: {
      let next = outer()
      return { () -> T? in
        while let x = next() {
          if p(x) { return x }
        }
        return nil
      }
    })
  }

  func compactMap<U>(_ f: (T) -> U?) -> LazySequence<U> {
    let outer = self.make
    return LazySequence<U>(make: {
      let next = outer()
      return { () -> U? in
        while let x = next() {
          if let y = f(x) { return y }
        }
        return nil
      }
    })
  }

  func prefix(_ n: Int) -> LazySequence<T> {
    let outer = self.make
    return LazySequence<T>(make: {
      let next = outer()
      var taken = 0
      return { () -> T? in
        if taken >= n { return nil }
        taken += 1
        return next()
      }
    })
  }

  func prefixWhile(while p: (T) -> Bool) -> LazySequence<T> {
    let outer = self.make
    return LazySequence<T>(make: {
      let next = outer()
      var done = false
      return { () -> T? in
        if done { return nil }
        if let x = next() {
          if p(x) { return x }
        }
        done = true
        return nil
      }
    })
  }

  func dropFirst(_ n: Int) -> LazySequence<T> {
    let outer = self.make
    return LazySequence<T>(make: {
      let next = outer()
      var skipped = 0
      return { () -> T? in
        while skipped < n {
          skipped += 1
          if next() == nil { return nil }
        }
        return next()
      }
    })
  }

  func dropWhile(while p: (T) -> Bool) -> LazySequence<T> {
    let outer = self.make
    return LazySequence<T>(make: {
      let next = outer()
      var dropping = true
      return { () -> T? in
        while dropping {
          guard let x = next() else { return nil }
          if !p(x) {
            dropping = false
            return x
          }
        }
        return next()
      }
    })
  }

  func enumerated() -> LazySequence<(offset: Int, element: T)> {
    let outer = self.make
    return LazySequence<(offset: Int, element: T)>(make: {
      let next = outer()
      var index = 0
      return { () -> (offset: Int, element: T)? in
        guard let x = next() else { return nil }
        let pair = (offset: index, element: x)
        index += 1
        return pair
      }
    })
  }

  func firstWhere(where p: (T) -> Bool) -> T? {
    let next = self.make()
    while let x = next() {
      if p(x) { return x }
    }
    return nil
  }

  func containsWhere(where p: (T) -> Bool) -> Bool {
    let next = self.make()
    while let x = next() {
      if p(x) { return true }
    }
    return false
  }

  func allSatisfy(_ p: (T) -> Bool) -> Bool {
    let next = self.make()
    while let x = next() {
      if !p(x) { return false }
    }
    return true
  }

  func reduce<R>(_ initial: R, _ f: (R, T) -> R) -> R {
    let next = self.make()
    var acc = initial
    while let x = next() {
      acc = f(acc, x)
    }
    return acc
  }

  func forEach(_ f: (T) -> Void) {
    let next = self.make()
    while let x = next() {
      f(x)
    }
  }
}

func _lazyArray<T>(_ a: [T]) -> LazySequence<T> {
  return LazySequence<T>(make: {
    var i = 0
    return { () -> T? in
      if i < a.count {
        let x = a[i]
        i += 1
        return x
      }
      return nil
    }
  })
}

func _lazyFirst<T>(_ s: LazySequence<T>) -> T? {
  let next = s.make()
  return next()
}

func _lazyCount<T>(_ s: LazySequence<T>) -> Int {
  let next = s.make()
  var n = 0
  while next() != nil { n += 1 }
  return n
}

func _lazyIsEmpty<T>(_ s: LazySequence<T>) -> Bool {
  let next = s.make()
  return next() == nil
}

func _lazyToArray<T>(_ s: LazySequence<T>) -> [T] {
  let next = s.make()
  var out: [T] = []
  while let x = next() { out.append(x) }
  return out
}
