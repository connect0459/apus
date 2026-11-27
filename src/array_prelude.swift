func _arrayPrefix<T>(_ a: [T], _ n: Int) -> [T] {
  if n < 0 { fatalError("prefix needs a count that is not negative") }
  var out: [T] = []
  var i = 0
  while i < n && i < a.count {
    out.append(a[i])
    i += 1
  }
  return out
}

func _arraySuffix<T>(_ a: [T], _ n: Int) -> [T] {
  if n < 0 { fatalError("suffix needs a count that is not negative") }
  var out: [T] = []
  var i = a.count - n
  if i < 0 { i = 0 }
  while i < a.count {
    out.append(a[i])
    i += 1
  }
  return out
}

func _arrayDropFirst<T>(_ a: [T], _ n: Int) -> [T] {
  if n < 0 { fatalError("dropFirst needs a count that is not negative") }
  var out: [T] = []
  var i = n
  while i < a.count {
    out.append(a[i])
    i += 1
  }
  return out
}

func _arrayDropLast<T>(_ a: [T], _ n: Int) -> [T] {
  if n < 0 { fatalError("dropLast needs a count that is not negative") }
  var out: [T] = []
  var i = 0
  while i < a.count - n {
    out.append(a[i])
    i += 1
  }
  return out
}

func _arrayReversed<T>(_ a: [T]) -> [T] {
  var out: [T] = []
  var i = a.count - 1
  while i >= 0 {
    out.append(a[i])
    i -= 1
  }
  return out
}

func _arrayPrefixWhile<T>(_ a: [T], while p: (T) -> Bool) -> [T] {
  var out: [T] = []
  for x in a {
    if !p(x) { break }
    out.append(x)
  }
  return out
}

func _arrayDropWhile<T>(_ a: [T], while p: (T) -> Bool) -> [T] {
  var out: [T] = []
  var dropping = true
  for x in a {
    if dropping && p(x) { continue }
    dropping = false
    out.append(x)
  }
  return out
}

func _stringPrefixWhile(_ s: String, while p: (Character) -> Bool) -> String {
  var out = ""
  for c in s {
    if !p(c) { break }
    out += String(c)
  }
  return out
}

func _stringDropWhile(_ s: String, while p: (Character) -> Bool) -> String {
  var out = ""
  var dropping = true
  for c in s {
    if dropping && p(c) { continue }
    dropping = false
    out += String(c)
  }
  return out
}
