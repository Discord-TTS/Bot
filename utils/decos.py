from functools import wraps

def wrap_with(enterable, aenter):
    def deco_wrap(func):
        async def async_wrapper(*args, **kwargs):
            async with enterable() as entered:
                return await func(entered, *args, **kwargs)

        async def normal_wrapper(*args, **kwargs):
            with enterable() as entered:
                return await func(entered, *args, **kwargs)

        return wraps(func)(async_wrapper if aenter else normal_wrapper)
    return deco_wrap

def handle_errors(func):
    async def wrapper(self, *args, **kwargs):
        try:
            return await func(self, *args, **kwargs)
        except Exception as error:
            return await self.bot.on_error(func.__name__, error)
    
    return wrapper
