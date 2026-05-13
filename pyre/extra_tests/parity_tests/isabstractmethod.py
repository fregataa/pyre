"""Phase 6 parity test: __isabstractmethod__ on staticmethod / classmethod.

PyPy:
  typedef.py:872  StaticMethod  __isabstractmethod__ = GetSetProperty(
                                  StaticMethod.descr_isabstract)
  typedef.py:886  ClassMethod   __isabstractmethod__ = GetSetProperty(
                                  ClassMethod.descr_isabstract)

  function.py:705-706 / 760-761:
      def descr_isabstract(self, space):
          return space.newbool(space.isabstractmethod_w(self.w_function))

  baseobjspace.py:1346-1353 isabstractmethod_w:
      try:
          w_result = self.getattr(w_obj, self.newtext("__isabstractmethod__"))
      except OperationError as e:
          if e.match(self, self.w_AttributeError):
              return False
          raise
      return self.is_true(w_result)

Pyre route: typedef descr_isabstract -> baseobjspace.isabstractmethod_w
-> getattr -> is_true.  Result is a real Python bool.
"""

# Plain function: no __isabstractmethod__ slot -> AttributeError caught -> False.
def _f():
    return 1

_sm = staticmethod(_f)
_cm = classmethod(_f)

assert _sm.__isabstractmethod__ is False, (
    f"plain function staticmethod: {_sm.__isabstractmethod__!r}"
)
assert _cm.__isabstractmethod__ is False, (
    f"plain function classmethod: {_cm.__isabstractmethod__!r}"
)

# Function with __isabstractmethod__ set: surfaces through.
def _g():
    return 2

_g.__isabstractmethod__ = True

_sm_g = staticmethod(_g)
_cm_g = classmethod(_g)
assert _sm_g.__isabstractmethod__ is True, (
    f"abstract-marked function staticmethod: {_sm_g.__isabstractmethod__!r}"
)
assert _cm_g.__isabstractmethod__ is True, (
    f"abstract-marked function classmethod: {_cm_g.__isabstractmethod__!r}"
)

# Falsy mark (set then unset): observed via is_true on the fetched value.
_g.__isabstractmethod__ = False
assert _sm_g.__isabstractmethod__ is False
assert _cm_g.__isabstractmethod__ is False

# Truthy but non-bool: still surfaces True (PyPy's is_true semantics).
_g.__isabstractmethod__ = 1
assert _sm_g.__isabstractmethod__ is True
assert _cm_g.__isabstractmethod__ is True

# Type: must be exactly bool, not int.
assert type(_sm.__isabstractmethod__) is bool
assert type(_cm.__isabstractmethod__) is bool

print("OK")
