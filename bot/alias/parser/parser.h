#ifndef PARSER_H
#define PARSER_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct ParseData {
  const char *prefix;
  const char *sub_command;
  const void *args;
  uint32_t args_length;
} ParseData;

typedef struct ParseResult {
  bool ok;
  bool data_available;
  struct ParseData data;
  const char *error_msg;
} ParseResult;

void free_parse_result(struct ParseResult result);

/**
 * ptr must be the pointer of parse function returned
 * or cause undefined behavior.
 */
const char *args_get_at(const void *ptr, uintptr_t pos);

struct ParseResult parse(const char *text_raw);

#endif /* PARSER_H */
