module;
#include <libintl.h>
export module i18n;

export inline const char* _(const char* msgid) {
    return gettext(msgid);
}
