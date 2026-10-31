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

profile-hardware = تفضيل الأداء
    .description = توقيت الإطارات وميزانية المؤثرات
    .auto = تلقائي
    .high-refresh = تحديث عالي
    .balanced = متوازن
    .legacy-low-spec = أجهزة قديمة
    .mobile-battery-saver = موفر طاقة البطارية

## [display]

display-frame_rate_cap = حد معدل الإطارات
    .description = أقصى معدل إطارات أثناء الحركة (0 يتبع تفضيل الأداء)
display-ui_scale = حجم الواجهة
    .description = مقياس الواجهة بالإضافة إلى مقياس النظام
display-interface = الواجهة
    .description = التخطيط: تلقائي يتبع الجهاز (تخطيطات اللمس على الأجهزة اللوحية والهواتف)
    .auto = تلقائي
    .desktop = سطح المكتب
    .tablet = لوحي
    .phone = هاتف
display-phone_frame = شكل هاتف
    .description = على الكمبيوتر، رسم تخطيط الهاتف في عمود بعرض الهاتف في منتصف النافذة
display-gpu_preference = تفضيل وحدة معالجة الرسوميات
    .description = المعالج الرسومي الذي يرسم النافذة؛ تلقائي يستخدم معالج الشاشة (يطبق بعد إعادة التشغيل)
    .auto = تلقائي
    .integrated = مدمج
    .discrete = منفصل

## [motion]

motion-instant_transitions = انتقالات فورية
    .description = تبديل الصفحات والقوائم في 0 مللي ثانية دون حركات
motion-animation_speed = سرعة الحركة
    .description = سرعة جميع حركات النوابض
motion-spring_stiffness = صلابة النابض
    .description = ثابت المرونة k لكل حركة
motion-spring_damping = تخميد النابض
    .description = معامل التخميد c لكل حركة
motion-settle_grace_ms = مهلة الاستقرار (مللي ثانية)
    .description = استمرار الرسم لهذه المدة بعد آخر تغيير (مللي ثانية)
motion-caret_blink_ms = وميض المؤشر (مللي ثانية)
    .description = الفاصل الزمني لوميض مؤشر النص (0 يبقيه ثابتاً)
motion-smooth_caret = مؤشر سلس
    .description = انزلاق مؤشر النص بسلاسة إلى موقعه الجديد بدلاً من القفز
motion-caret_glide_ms = مدة انزلاق المؤشر
    .description = المدة المستغرقة لانزلاق المؤشر إلى مكانه الجديد (مللي ثانية؛ الأقل أسرع)
motion-message_seconds = ثواني بقاء الرسالة
    .description = مدة بقاء الرسائل في شريط الحالة (ث)

## [layout]

layout-padding = الحشو
    .description = المسافة داخل الأشرطة والأزرار وعناصر التحكم (بكسل)
layout-border_width = عرض الحدود
    .description = سُمك الحدود الرفيعة (بكسل)
layout-control_radius = نصف قطر التحكم
    .description = تدوير زوايا الأزرار والحقول (بكسل)
layout-pill_radius = نصف قطر الأزرار البيضاوية
    .description = تدوير زوايا النوافذ والقوائم والأزرار البيضاوية (بكسل)
layout-pill_height = ارتفاع الزر البيضاوي
    .description = ارتفاع الأزرار البيضاوية (بكسل)
layout-status_bar = شريط الحالة
    .description = إظهار شريط الحالة الرفيع أسفل النافذة (بدونه تظهر الرسائل كتنبيهات)

## [colors]
colors-void = فراغ
    .description = خلفية الصفحة
colors-panel = لوحة
    .description = الأشرطة واللوحات
colors-pill = عنصر بيضاوي
    .description = القوائم والمربعات والأسطح العائمة
colors-hover = عند التمرير
    .description = عناصر التحكم تحت المؤشر
colors-active = نشط
    .description = عناصر التحكم المضغوطة والمختارة
colors-border = حد
    .description = الحدود الرفيعة
colors-border_active = حد نشط
    .description = حدود المربعات الممرر عليها وعناصر التحكم المفعلة
colors-text = نص
    .description = النص الأساسي والأيقونات النشطة
colors-muted = خافت
    .description = النص الثانوي
colors-selection = تحديد
    .description = تمييز تحديد النص
colors-selection_text = نص التحديد
    .description = النص داخل التحديد
colors-shadow = ظل
    .description = الظل أسفل الأسطح العائمة

## [typography]

typography-ui_font = خط الواجهة
    .description = خط الواجهة المدمج
typography-ui_size = حجم الواجهة
    .description = حجم نص الواجهة (بكسل)
typography-small_size = نص صغير
    .description = حجم نص التسميات التوضيحية وشريط الحالة (بكسل)
typography-heading_size = حجم العناوين
    .description = حجم نص العناوين (بكسل)
typography-mono_size = حجم الخط أحادي المسافة
    .description = حجم النص أحادي المسافة (بكسل)

## [input]

## [io]

## [audio]

audio-buffer_frames = إطارات مخزن الصوت
    .description = العينات المرسلة لبطاقة الصوت في كل دفعة (0 للافتراضي)
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
sue_paint-dpi = الدقة
    .description = دقة الرسوم التوضيحية الجديدة (نقطة لكل بوصة)
sue_paint-materials_dir = مجلد المواد
    .description = مكان حفظ المواد ورؤوس الفرش في Sue Paint (فارغ: المجلد الخاص)
sue_paint-save_format = تنسيق الحفظ
    .description = طريقة حفظ الصورة: ملف clip. من Clip Studio Paint أو مشروع Sue
    .clip = Clip Studio Paint (.clip)
    .sue = مشروع Sue (.sue)
sue_paint-touch_mode = وضع اللمس
    .description = Sue Paint على الهواتف والأجهزة اللوحية: الوضع البسيط أو وضع الاستوديو
    .simple = بسيط
    .studio = استوديو
sue_paint-custom_key_1 = مفتاح مخصص 1
    .description = الأمر الذي ينفذه المفتاح T1 في وضع الاستوديو
sue_paint-custom_key_2 = مفتاح مخصص 2
    .description = الأمر الذي ينفذه المفتاح T2 في وضع الاستوديو
sue_paint-custom_key_3 = مفتاح مخصص 3
    .description = الأمر الذي ينفذه المفتاح T3 في وضع الاستوديو
sue_paint-custom_key_4 = مفتاح مخصص 4
    .description = الأمر الذي ينفذه المفتاح T4 في وضع الاستوديو
sue_paint-custom_key_5 = مفتاح مخصص 5
    .description = الأمر الذي ينفذه المفتاح T5 في وضع الاستوديو
sue_paint-custom_key_6 = مفتاح مخصص 6
    .description = الأمر الذي ينفذه المفتاح T6 في وضع الاستوديو
sue_paint-custom_key_7 = مفتاح مخصص 7
    .description = الأمر الذي ينفذه المفتاح T7 في وضع الاستوديو
sue_paint-theme = السمة
    .description = ألوان Sue Paint: رمادي، سمة Sue، مسبق الضبط أو مخصص
    .gray = رمادي
    .sue-theme = سمة Sue
    .void = فراغ
    .graphite = جرافيت
    .paper = ورق
    .custom = مخصص
sue_paint-pen_pressure = ضغط القلم
    .description = منحنى ضغط القلم لكل أداة (ملف -> ضبط مستوى ضغط القلم)
sue_paint-shift_line = Shift للخطوط المستقيمة
    .description = Shift مع السحب يرسم خطاً مستقيماً أثناء السحب (تفضيلات -> أدوات)
sue_paint-two_finger_tap = نقر بإصبعين
    .description = ما يفعله النقر السريع بإصبعين على اللوحة (تفضيلات -> إيماءات اللمس)
    .undo = تراجع
    .none = لا شيء
sue_paint-three_finger_tap = نقر بثلاثة أصابع
    .description = ما يفعله النقر السريع بثلاثة أصابع على اللوحة (تفضيلات -> إيماءات اللمس)
    .redo = إعادة
    .none = لا شيء
sue_paint-brush_cursor = مؤشر الفرشاة
    .description = المؤشر على اللوحة أثناء الرسم: مخطط الفرشاة أو علامة زائد أو نقطة (تفضيلات -> المؤشر)
    .brush = شكل الفرشاة
    .crosshair = علامة زائد
    .dot = نقطة
sue_paint-selection_layer_opacity = عتامة طبقات التحديد
    .description = عتامة طبقة التحديد الجديدة (٪؛ تفضيلات -> طبقة/إطار)
sue_paint-rotate_step = خطوة التدوير
    .description = مقدار دوران العرض عند التدوير لليسار أو اليمين (درجات؛ تفضيلات -> لوحة الرسم)
sue_paint-alt_tool = أداة Alt
    .description = ما يفعله Alt أثناء الرسم على اللوحة (ملف -> إعدادات مفاتيح التعديل)
    .eyedropper = قطارة
    .eraser = ممحاة
    .none = لا شيء
sue_paint-space_tool = أداة المسافة
    .description = ما يفعله مفتاح المسافة على اللوحة (ملف -> إعدادات مفاتيح التعديل)
    .hand = يد
    .none = لا شيء
sue_paint-ctrl_shift = Ctrl+Shift
    .description = ما ينفذه النقر مع Ctrl+Shift على اللوحة (ملف -> إعدادات مفاتيح التعديل)
    .pick_layer = اختيار طبقة
    .none = لا شيء
sue_paint-language = اللغة
    .description = اللغة: sue تتبع لغة Sue، أو علامة لغة لتمييز هذا الاستوديو

## [sue_paint_colors]

sue_paint_colors-void = فراغ
    .description = حول الصورة
sue_paint_colors-panel = لوحة
    .description = اللوحات والقوائم
sue_paint_colors-pill = عنصر بيضاوي
    .description = الحقول والرقاقات والبطاقات
sue_paint_colors-hover = عند التمرير
    .description = عناصر التحكم تحت المؤشر
sue_paint_colors-active = نشط
    .description = عناصر التحكم المضغوطة والمحددة
sue_paint_colors-border = حد
    .description = الحدود الرفيعة
sue_paint_colors-border_active = حد نشط
    .description = حدود عناصر التحكم المحددة والمفعلة
sue_paint_colors-text = نص
    .description = النص الأساسي والأيقونات النشطة
sue_paint_colors-muted = خافت
    .description = النص الثانوي
sue_paint_colors-selection = تحديد
    .description = تمييز تحديد النص
sue_paint_colors-selection_text = نص التحديد
    .description = النص داخل التحديد
sue_paint_colors-grid = شبكة
    .description = الشبكات والمساطر
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
sue_writer-language = اللغة
    .description = اللغة: sue تتبع لغة Sue، أو علامة لغة لتمييز هذا الاستوديو

## [image_viewer]

image_viewer-language = اللغة
    .description = اللغة: sue تتبع لغة Sue، أو علامة لغة لتمييز هذا الاستوديو

## [convert]

convert-language = اللغة
    .description = اللغة: sue تتبع لغة Sue، أو علامة لغة لتمييز هذا الاستوديو

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

## [image_viewer_keys]

image_viewer_keys-viewer_next = عارض التالي
    .description = الصورة التالية
image_viewer_keys-viewer_previous = عارض السابق
    .description = الصورة السابقة
image_viewer_keys-viewer_fit = عارض ملاءمة
    .description = ملاءمة للنافذة
image_viewer_keys-viewer_actual = عارض الحجم الفعلي
    .description = الحجم الفعلي
image_viewer_keys-viewer_zoom_in = عارض تكبير
    .description = تكبير
image_viewer_keys-viewer_zoom_out = عارض تصغير
    .description = تصغير
image_viewer_keys-viewer_rotate_right = عارض تدوير لليمين
    .description = تدوير لليمين
image_viewer_keys-viewer_rotate_left = عارض تدوير لليسار
    .description = تدوير لليسار
image_viewer_keys-viewer_crop = عارض قص
    .description = قص
image_viewer_keys-viewer_save_as = عارض حفظ باسم
    .description = حفظ باسم


layout-bar_height = ارتفاع الشريط
    .description = ارتفاع الشريط العلوي مع زر Sue والقوائم (بكسل؛ تخطيطات اللمس تجعله أطول)

layout-status_bar_height = ارتفاع شريط الحالة
    .description = ارتفاع شريط الحالة (بكسل)

layout-settings_sidebar_width = عرض الشريط الجانبي للإعدادات
    .description = عرض قائمة الفئات في الإعدادات (بكسل)

layout-shadow_blur = ضبابية الظل
    .description = ضبابية الظل أسفل القوائم والنوافذ (بكسل)

colors-grid = شبكة
    .description = خطوط الشبكة في محررات المنحنيات

input-wheel_line_px = خط عجلة التمرير (بكسل)
    .description = البكسلات التي يتم تمريرها لكل حركة لعجلة الفأرة في القوائم

io-jxl_effort = جهد JPEG XL
    .description = جهد JPEG XL للتحويلات (الأعلى أصغر وأبطأ)

io-jpeg_lossless_transcode = تحويل JPEG دون فقدان
    .description = إعادة ضغط JPEG إلى JPEG XL بدقة البت عند تثبيت برنامج ترميز

io-jpeg_quality = جودة JPEG
    .description = جودة JPEG للتحويلات إلى jpg.

io-byte_map_width = عرض خريطة البايتات
    .description = الأعمدة في خريطة البايتات للملفات غير المعروفة

io-max_ingest_mib = أقصى حجم للإدخال (ميغابايت)
    .description = أكبر ملف يمكن لـ Sue فتحه (ميغابايت)

io-convert_dir = مجلد التحويل
    .description = مجلد حفظ الملفات المحولة (الفارغ يحفظ بجانب المصدر)

audio-output_device = جهاز الإخراج
    .description = جهاز إخراج الصوت (الفارغ يستخدم الافتراضي للنظام)

audio-realtime_priority = أولوية الوقت الفعلي
    .description = طلب جدولة مسار الصوت في الوقت الفعلي من نظام التشغيل

audio-volume = مستوى الصوت
    .description = مستوى صوت كل ما يشغله Sue

packages-require_signatures = طلب التوقيعات
    .description = قبول الحزم ذات توقيع Ed25519 الصالح فقط
