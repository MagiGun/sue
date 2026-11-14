## إعدادات Sue (the rice). مسودة: بانتظار مراجعة متحدث أصلي.

## [general]

general-confirm_on_exit = تأكيد عند الخروج
    .description = السؤال قبل إغلاق النافذة
general-language = اللغة
    .description = لغة Sue الرئيسية التي يتبعها كل استوديو وأداة ما لم تخصص (النظام يتبع لغة النظام)
general-autosave_seconds = الحفظ التلقائي كل
    .description = حفظ العمل غير المحفوظ بهذا المعدل لاستعادته من المركز (ث؛ 0 يعطل الحفظ)

## [launchpad]

launchpad-recent_count = الملفات الأخيرة في المركز
    .description = عدد الملفات والمشاريع الأخيرة المعروضة في المركز (0 يخفي القائمة)
launchpad-pinned = الفتح عند البدء
    .description = الاستوديو أو الأداة التي يفتح عليها Sue مباشرة (الدبوس في المركز؛ الفارغ يفتح المركز)

## [profile]


## [display]


## [motion]

motion-instant_transitions = انتقالات فورية
    .description = تبديل الصفحات والقوائم في 0 مللي ثانية دون حركات
motion-animation_speed = سرعة الحركة
    .description = سرعة جميع حركات النوابض
motion-caret_blink_ms = وميض المؤشر (مللي ثانية)
    .description = الفاصل الزمني لوميض مؤشر النص (0 يبقيه ثابتاً)
motion-smooth_caret = مؤشر سلس
    .description = انزلاق مؤشر النص بسلاسة إلى موقعه الجديد بدلاً من القفز
motion-caret_glide_ms = مدة انزلاق المؤشر
    .description = المدة المستغرقة لانزلاق المؤشر إلى مكانه الجديد (مللي ثانية؛ الأقل أسرع)

## [layout]

layout-padding = الحشو
    .description = المسافة داخل الأشرطة والأزرار وعناصر التحكم (بكسل)
layout-status_bar = شريط الحالة
    .description = إظهار شريط الحالة الرفيع أسفل النافذة (بدونه تظهر الرسائل كتنبيهات)

## [colors]
colors-text = نص
    .description = النص الأساسي والأيقونات النشطة
colors-shadow = ظل
    .description = الظل أسفل الأسطح العائمة

## [typography]


## [input]

## [io]

## [audio]

audio-sample_rate = معدل عينة الصوت
    .description = المعدل الداخلي لمعالجة الصوت (هرتز؛ 0 للافتراضي)
## [packages]
## [updates]
updates-check_automatically = تحقق تلقائياً
    .description = البحث عن إصدار أحدث من Sue عند البدء (مرة واحدة يومياً كحد أقصى)

## [sue_paint]

sue_paint-width = العرض
    .description = عرض الرسوم التوضيحية الجديدة (بكسل)
sue_paint-height = الارتفاع
    .description = ارتفاع الرسوم التوضيحية الجديدة (بكسل)
sue_paint-save_format = تنسيق الحفظ
    .description = طريقة حفظ الصورة: ملف clip. من Clip Studio Paint أو مشروع Sue
    .clip = Clip Studio Paint (.clip)
    .sue = مشروع Sue (.sue)
sue_paint-theme = السمة
    .description = ألوان Sue Paint: رمادي، سمة Sue، مسبق الضبط أو مخصص
    .gray = رمادي
    .sue-theme = سمة Sue
    .void = فراغ
    .graphite = جرافيت
    .paper = ورق
    .custom = مخصص

## [sue_paint_colors]

sue_paint_colors-text = نص
    .description = النص الأساسي والأيقونات النشطة
sue_paint_colors-shadow = ظل
    .description = الظل أسفل الأسطح العائمة

## [sue_writer]

sue_writer-theme = السمة
    .description = ألوان Sue Writer: سمة Sue (تتبعها) أو مسبق ضبط
    .sue-theme = سمة Sue
    .gray = رمادي
    .void = فراغ
    .graphite = جرافيت
    .paper = ورق
sue_writer-smart_quotes = علامات اقتباس ذكية
    .description = تحويل علامات الاقتباس المستقيمة إلى منحنية أثناء الكتابة
sue_writer-dark_paper = ورق داكن
    .description = إظهار الصفحة بالألوان الداكنة للسمة بدلاً من الورق الأبيض
sue_writer-spell_check = التدقيق الإملائي
    .description = وضع خط تحت الكلمات التي تحتوي على أخطاء إملائية أثناء الكتابة

## [keys]

keys-command_palette = لوحة الأوامر
    .description = فتح لوحة الأوامر
keys-command_palette_alt = لوحة الأوامر بديل
    .description = فتح لوحة الأوامر (بديل)
keys-open_settings = فتح الإعدادات
    .description = فتح الإعدادات
keys-open_file = فتح ملف
    .description = فتح ملف
keys-save = حفظ
    .description = حفظ المشروع
keys-quit = خروج
    .description = إنهاء Sue
keys-undo = تراجع
    .description = تراجع عن آخر تغيير
keys-redo = إعادة
    .description = إعادة
keys-redo_alt = إعادة بديل
    .description = إعادة (بديل)

## [sue_paint_keys]

sue_paint_keys-help_shortcuts = مساعدة الاختصارات
    .description = إعدادات اختصارات لوحة المفاتيح
sue_paint_keys-file_preferences = ملف التفضيلات
    .description = التفضيلات
sue_paint_keys-file_save_as = ملف حفظ باسم
    .description = حفظ باسم
sue_paint_keys-file_print = ملف طباعة
    .description = طباعة
sue_paint_keys-layer_new = طبقة جديدة
    .description = طبقة نقطية
sue_paint_keys-file_new = ملف جديد
    .description = جديد
sue_paint_keys-file_close = ملف إغلاق
    .description = إغلاق
sue_paint_keys-edit_transform_free = تحرير تحويل حر
    .description = تحويل حر
sue_paint_keys-edit_transform_scale = تحرير تحويل قياس
    .description = تغيير الحجم/تدوير
sue_paint_keys-filter_hue = مرشح صبغة
    .description = الصبغة/التشبع/الإضاءة
sue_paint_keys-select_invert = تحديد عكس
    .description = عكس المنطقة المحددة
sue_paint_keys-filter_invert = مرشح عكس
    .description = عكس التدرج
sue_paint_keys-edit_cut = تحرير قص
    .description = قص
sue_paint_keys-edit_copy = تحرير نسخ
    .description = نسخ
sue_paint_keys-edit_paste = تحرير لصق
    .description = لصق
sue_paint_keys-edit_clear = تحرير مسح
    .description = حذف
sue_paint_keys-edit_fill = تحرير ملء
    .description = ملء
sue_paint_keys-layer_toggle_clip = طبقة تبديل القص
    .description = قص إلى الطبقة السفلية
sue_paint_keys-layer_folder_insert = طبقة إدراج مجلد
    .description = إنشاء مجلد وإدراج طبقة
sue_paint_keys-layer_merge_visible_here = طبقة دمج المرئي هنا
    .description = دمج الطبقات المرئية
sue_paint_keys-layer_ungroup = طبقة فك تجميع
    .description = فك تجميع مجلد الطبقات
sue_paint_keys-layer_merge_selected = طبقة دمج المحدد
    .description = دمج الطبقات المحددة
sue_paint_keys-layer_merge_down = طبقة دمج لأسفل
    .description = دمج مع الطبقة السفلية
sue_paint_keys-layer_select_above = طبقة تحديد لأعلى
    .description = تحديد الطبقة العلوية
sue_paint_keys-layer_select_below = طبقة تحديد لأسفل
    .description = تحديد الطبقة السفلية
sue_paint_keys-layer_order_up = طبقة تحريك لأعلى
    .description = تحريك لأعلى
sue_paint_keys-layer_order_down = طبقة تحريك لأسفل
    .description = تحريك لأسفل
sue_paint_keys-select_again = تحديد إعادة
    .description = إعادة التحديد
sue_paint_keys-select_all = تحديد الكل
    .description = تحديد الكل
sue_paint_keys-select_none = تحديد إلغاء
    .description = إلغاء التحديد
sue_paint_keys-view_actual = عرض الحجم الفعلي
    .description = 100%
sue_paint_keys-view_fit = عرض ملاءمة
    .description = ملاءمة للشاشة
sue_paint_keys-view_rotate_reset = عرض إعادة ضبط التدوير
    .description = إعادة ضبط التدوير
sue_paint_keys-view_zoom_in = عرض تكبير
    .description = تكبير
sue_paint_keys-view_zoom_out = عرض تصغير
    .description = تصغير
sue_paint_keys-view_ruler_bar = عرض شريط المساطر
    .description = شريط المساطر
sue_paint_keys-view_snap_ruler = عرض محاذاة للمسطرة
    .description = محاذاة للمسطرة
sue_paint_keys-view_snap_special = عرض محاذاة لمسطرة خاصة
    .description = محاذاة لمسطرة خاصة
sue_paint_keys-view_snap_grid = عرض محاذاة للشبكة
    .description = محاذاة للشبكة
sue_paint_keys-view_rotate_left = عرض تدوير لليسار
    .description = تدوير لليسار
sue_paint_keys-view_rotate_right = عرض تدوير لليمين
    .description = تدوير لليمين
sue_paint_keys-window_prev_tab = نافذة اللوحة السابقة
    .description = اللوحة السابقة
sue_paint_keys-window_next_tab = نافذة اللوحة التالية
    .description = اللوحة التالية
sue_paint_keys-window_hide_menus = نافذة إخفاء القوائم
    .description = إخفاء شريط القوائم
sue_paint_keys-window_hide_all = نافذة إخفاء الكل
    .description = إخفاء جميع اللوحات
sue_paint_keys-tool_zoom = أداة تكبير
    .description = أداة التكبير
sue_paint_keys-tool_hand = أداة يد
    .description = أداة اليد
sue_paint_keys-tool_rotate = أداة تدوير
    .description = أداة التدوير
sue_paint_keys-tool_operation = أداة عملية
    .description = أداة العمليات
sue_paint_keys-tool_move_layer = أداة تحريك الطبقة
    .description = أداة تحريك الطبقة
sue_paint_keys-tool_selection = أداة تحديد
    .description = أداة التحديد
sue_paint_keys-tool_auto_select = أداة تحديد تلقائي
    .description = أداة التحديد التلقائي
sue_paint_keys-tool_eyedropper = أداة قطارة
    .description = أداة القطارة
sue_paint_keys-tool_pen = أداة قلم
    .description = أدوات القلم وقلم الرصاص
sue_paint_keys-tool_brush = أداة فرشاة
    .description = أدوات الفرشاة والرشاش والزخرفة
sue_paint_keys-tool_eraser = أداة ممحاة
    .description = أداة الممحاة
sue_paint_keys-tool_blend = أداة مزج
    .description = أداة مزج الألوان
sue_paint_keys-tool_fill = أداة ملء
    .description = أدوات الملء والتدرج
sue_paint_keys-tool_figure = أداة شكل
    .description = أدوات الأشكال وإطارات القصص والمساطر
sue_paint_keys-tool_text = أداة نص
    .description = أدوات النصوص وبالونات الحوار
sue_paint_keys-tool_correct_line = أداة تصحيح الخط
    .description = أداة تصحيح الخط
sue_paint_keys-tool_previous_sub = أداة الأداة الفرعية السابقة
    .description = الأداة الفرعية السابقة
sue_paint_keys-tool_next_sub = أداة الأداة الفرعية التالية
    .description = الأداة الفرعية التالية
sue_paint_keys-color_swap = تبديل الألوان
    .description = تبديل اللون الرئيسي والثانوي
sue_paint_keys-color_transparent = لون شفاف
    .description = لون الرسم أو الشفاف
sue_paint_keys-brush_smaller = فرشاة أصغر
    .description = تصغير حجم الفرشاة
sue_paint_keys-brush_bigger = فرشاة أكبر
    .description = تكبير حجم الفرشاة

## [sue_writer_keys]

sue_writer_keys-binder_new_folder = مصنف مجلد جديد
    .description = مجلد جديد
sue_writer_keys-binder_new_doc = مصنف مستند جديد
    .description = مستند جديد
sue_writer_keys-compile_open = ترجمة فتح
    .description = ترجمة
sue_writer_keys-compile_pdf = ترجمة PDF
    .description = PDF
sue_writer_keys-format_bold = تنسيق عريض
    .description = عريض
sue_writer_keys-format_italic = تنسيق مائل
    .description = مائل
sue_writer_keys-format_underline = تنسيق تسطير
    .description = تسطير
sue_writer_keys-format_strike = تنسيق شطب
    .description = يتوسطه خط
sue_writer_keys-project_split = مشروع تقسيم
    .description = تقسيم عند التحديد
sue_writer_keys-format_superscript = تنسيق مرتفع
    .description = مرتفع
sue_writer_keys-format_subscript = تنسيق منخفض
    .description = منخفض
sue_writer_keys-format_clear = تنسيق مسح
    .description = مسح التنسيق المباشر
sue_writer_keys-align_center = محاذاة وسط
    .description = توسيط
sue_writer_keys-align_left = محاذاة يسار
    .description = يسار
sue_writer_keys-align_right = محاذاة يمين
    .description = يمين
sue_writer_keys-align_justify = محاذاة ضبط
    .description = ضبط
sue_writer_keys-style_heading1 = نمط عنوان 1
    .description = عنوان 1
sue_writer_keys-style_heading2 = نمط عنوان 2
    .description = عنوان 2
sue_writer_keys-style_heading3 = نمط عنوان 3
    .description = عنوان 3
sue_writer_keys-style_body = نمط متن
    .description = متن النص
sue_writer_keys-insert_footnote = إدراج حاشية سفلية
    .description = حاشية سفلية
sue_writer_keys-insert_page_break = إدراج فاصل صفحات
    .description = فاصل صفحات
sue_writer_keys-insert_link = إدراج رابط
    .description = رابط
sue_writer_keys-edit_find = تحرير بحث
    .description = بحث
sue_writer_keys-edit_replace = تحرير استبدال
    .description = استبدال
sue_writer_keys-edit_find_previous = تحرير بحث عن السابق
    .description = بحث عن السابق
sue_writer_keys-edit_find_next = تحرير بحث عن التالي
    .description = بحث عن التالي
sue_writer_keys-view_composition = عرض وضع التركيز
    .description = وضع التركيز
sue_writer_keys-view_document = عرض مستند
    .description = نص
sue_writer_keys-view_corkboard = عرض لوحة ملاحظات
    .description = لوحة ملاحظات
sue_writer_keys-view_outliner = عرض مخطط
    .description = مخطط
sue_writer_keys-project_snapshot = مشروع لقطة
    .description = التقاط لقطة
sue_writer_keys-list_bullet = قائمة نقطية
    .description = قائمة نقطية
sue_writer_keys-table_insert = إدراج جدول
    .description = إدراج جدول
sue_writer_keys-custom = مخصصة
    .description = مفاتيح الأوامر التي ليس لها مفتاح افتراضي
sue_writer_keys-review_comment = مراجعة تعليق
    .description = تعليق جديد
sue_writer_keys-insert_endnote = إدراج تعليق ختامي
    .description = تعليق ختامي
sue_writer_keys-review_track = مراجعة تتبع التغييرات
    .description = تسجيل التغييرات
sue_writer_keys-review_next_misspelling = مراجعة الخطأ الإملائي التالي
    .description = الخطأ الإملائي التالي
sue_writer_keys-format_grow = تنسيق تكبير الخط
    .description = تكبير الخط
sue_writer_keys-format_shrink = تنسيق تصغير الخط
    .description = تصغير الخط
sue_writer_keys-insert_mark_nbsp = إدراج مسافة غير منقسمة
    .description = مسافة غير منقسمة

## [layout]



layout-status_bar_height = ارتفاع شريط الحالة
    .description = ارتفاع شريط الحالة (بكسل)


layout-shadow_blur = ضبابية الظل
    .description = ضبابية الظل أسفل القوائم والنوافذ (بكسل)





io-jpeg_quality = جودة JPEG
    .description = جودة JPEG عند التصدير إلى jpg.




audio-output_device = جهاز الإخراج
    .description = جهاز إخراج الصوت (الفارغ يستخدم الافتراضي للنظام)


audio-volume = مستوى الصوت
    .description = مستوى صوت كل ما يشغله Sue

