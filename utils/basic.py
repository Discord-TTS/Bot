def get_value(dictionary, *nested_values, default_value = None):
    try:
        for value in nested_values:
            dictionary = dictionary[value]
    except (TypeError, AttributeError, KeyError):
        return default_value

    return dictionary

def remove_chars(remove_from, *chars):
    input_string = str(remove_from)
    for char in chars:  input_string = input_string.replace(char, "")

    return input_string

def sort_dict(dict_to_sort):
    keys = list(dict_to_sort.keys())
    keys.sort()
    newdict = {}
    for x in keys:
        newdict[x] = dict_to_sort[x]

    return newdict
