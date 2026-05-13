"""Phase 6 parity test: descriptor `__get__` API for staticmethod / classmethod.

PyPy bindings:
  - `function.py:691-693 StaticMethod.descr_staticmethod_get`:

        def descr_staticmethod_get(self, w_obj, w_cls=None):
            return self.w_function

  - `function.py:738-748 ClassMethod.descr_classmethod_get`:

        def descr_classmethod_get(self, space, w_obj, w_klass=None):
            if space.is_none(w_klass):
                w_klass = space.type(w_obj)
            w_func = self.w_function
            w_bound = space.get(w_func, w_klass, w_klass)
            if w_bound is not w_func:
                return w_bound
            return Method(space, w_func, w_klass)

Typedef registration: `typedef.py:866` and `:883` register
`__get__ = interp2app(...)` so user code can call the descriptor
directly without going through `obj.attr` attribute lookup.

Pinned contract:
  1. staticmethod's `__get__(None, C)` returns the wrapped function
     (no implicit first argument).
  2. staticmethod's `__get__(instance, C)` also returns the wrapped
     function (instance is ignored).
  3. classmethod's `__get__(None, C)` returns a bound `method` whose
     receiver is the class.
  4. classmethod's `__get__(instance, C)` likewise returns a bound
     method to the class — not the instance.
  5. classmethod's `__get__(None, C)` with `w_klass=None` would fill in
     `type(obj)` per `:739-740`; we don't directly exercise that
     edge here because the typedef requires the class arg explicitly.
"""

class C:
    sm = staticmethod(lambda x: x * 2)
    cm = classmethod(lambda cls, x: (cls.__name__, x))


# Reach the raw descriptors via the class __dict__ so the lookup does
# NOT itself invoke __get__ — needed to exercise `descriptor.__get__`
# as a user-callable method.
raw_sm = C.__dict__['sm']
raw_cm = C.__dict__['cm']

assert type(raw_sm).__name__ == "staticmethod"
assert type(raw_cm).__name__ == "classmethod"


# (1) staticmethod.__get__(None, C) → wrapped function.
f1 = raw_sm.__get__(None, C)
assert f1(5) == 10, f"sm(None, C): {f1(5)!r}"


# (2) staticmethod.__get__(instance, C) — instance ignored.
f2 = raw_sm.__get__(C(), C)
assert f2(5) == 10


# (3) classmethod.__get__(None, C) → bound method to the class.
m1 = raw_cm.__get__(None, C)
assert type(m1).__name__ == "method", f"cm wrapper type: {type(m1).__name__!r}"
assert m1(7) == ("C", 7), f"m1(7): {m1(7)!r}"


# (4) classmethod.__get__(instance, C) → bound method to the class.
m2 = raw_cm.__get__(C(), C)
assert type(m2).__name__ == "method"
assert m2(9) == ("C", 9), f"m2(9): {m2(9)!r}"


# (5) Calling via type(descriptor).__get__ (unbound form) — same result.
f3 = type(raw_sm).__get__(raw_sm, None, C)
assert f3(5) == 10
m3 = type(raw_cm).__get__(raw_cm, None, C)
assert m3(11) == ("C", 11)


# Confirm the descriptor protocol still fires through attribute lookup
# (i.e. `C.sm` / `C.cm` still match `raw_sm.__get__(None, C)`).
assert C.sm(5) == raw_sm.__get__(None, C)(5)
assert C.cm(5) == raw_cm.__get__(None, C)(5)


print("OK")
