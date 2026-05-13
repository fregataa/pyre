"""Phase 6 parity test: staticmethod / classmethod descriptor protocol.

PyPy `function.py:691-693 StaticMethod.descr_staticmethod_get`:

    def descr_staticmethod_get(self, w_obj, w_cls=None):
        return self.w_function

PyPy `function.py:738-748 ClassMethod.descr_classmethod_get`:

    def descr_classmethod_get(self, space, w_obj, w_klass=None):
        if space.is_none(w_klass):
            w_klass = space.type(w_obj)
        w_func = self.w_function
        w_bound = space.get(w_func, w_klass, w_klass)
        if w_bound is not w_func:
            return w_bound
        return Method(space, w_func, w_klass)

Typedef bindings: typedef.py:866 / :883 `__get__ = interp2app(...)`.

User-visible contract pinned here:
  - staticmethod accessed through class or instance calls the
    wrapped function with no implicit first argument.
  - classmethod accessed through class or instance calls the
    wrapped function with the class as implicit first argument.
  - classmethod surface yields a bound `method` (callable) wrapper.

NOT asserted: the descriptor's `__get__` method itself.  Pyre
currently routes staticmethod / classmethod attribute access
through a special-case in baseobjspace::getattr rather than
calling the descriptor's `__get__` slot directly; the protocol
API gap is a separate audit.
"""

class _C:
    sm = staticmethod(lambda x: x * 2)
    cm = classmethod(lambda cls, x: (cls.__name__, x))


# staticmethod: class or instance, called without self.
assert _C.sm(5) == 10
assert _C().sm(5) == 10


# classmethod: class or instance, receives the class as first arg.
assert _C.cm(5) == ("_C", 5), f"class call: {_C.cm(5)!r}"
assert _C().cm(5) == ("_C", 5), f"instance call: {_C().cm(5)!r}"


# classmethod surface: bound `method` wrapper.
assert type(_C.cm).__name__ == "method", (
    f"_C.cm wrapper: {type(_C.cm).__name__!r}"
)
assert type(_C().cm).__name__ == "method"


# A subclass inherits the classmethod and the class arg follows the
# call site, not the defining class.
class _D(_C):
    pass

assert _D.cm(7) == ("_D", 7), f"_D.cm: {_D.cm(7)!r}"
assert _D().cm(7) == ("_D", 7)


# staticmethod inherited subclass: still no self / cls injected.
assert _D.sm(9) == 18
assert _D().sm(9) == 18


# classmethod composed with a method-style def.
class _E:
    @classmethod
    def label(cls):
        return f"<{cls.__name__}>"

assert _E.label() == "<_E>"
assert _E().label() == "<_E>"


# staticmethod composed with a method-style def.
class _F:
    @staticmethod
    def double(x):
        return x + x

assert _F.double(3) == 6
assert _F().double(3) == 6

print("OK")
