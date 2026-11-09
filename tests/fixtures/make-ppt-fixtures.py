# -*- coding: utf-8 -*-
# PPT 요소 따로 고치기(pptxparse.js) 점검용 작은 .pptx 만들기 (python-pptx 1.x)
#   python3 tests/fixtures/make-ppt-fixtures.py  → tests/fixtures/elements.pptx (가상 데이터: 홍길동 · ○○교회)
# 슬라이드: ① 제목·부제 개체 틀(레이아웃·마스터 상속) ② 글상자·그림(자른 것)·둥근 사각형(글 포함)·타원·선·표
#          ③ 숨긴 슬라이드 ④ 단색 배경 + 그룹 도형 + 글머리표 ⑤ 그림 배경(슬라이드 가득) + 글
import io
import os
import struct
import zlib

from pptx import Presentation
from pptx.util import Pt, Emu
from pptx.dml.color import RGBColor
from pptx.enum.shapes import MSO_SHAPE, MSO_CONNECTOR
from pptx.enum.text import PP_ALIGN, MSO_ANCHOR
from pptx.enum.dml import MSO_THEME_COLOR

HERE = os.path.dirname(os.path.abspath(__file__))


def png(w, h, rgb):
    """단색 PNG (작게)"""
    raw = b''.join(b'\x00' + bytes(rgb) * w for _ in range(h))
    def chunk(t, d):
        return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b'')


def main():
    prs = Presentation()  # 기본 템플릿 4:3 (10in × 7.5in = 720 × 540 pt)
    prs.slide_width = Emu(12192000)  # 16:9 (960 × 540 pt)
    prs.slide_height = Emu(6858000)

    # ① 제목 슬라이드: 글자 크기·정렬을 레이아웃·마스터에서 물려받는다
    s1 = prs.slides.add_slide(prs.slide_layouts[0])
    s1.shapes.title.text = '주일 예배'
    s1.placeholders[1].text = '○○교회 · 2026년 11월'

    # ② 빈 레이아웃: 요소 여러 가지
    s2 = prs.slides.add_slide(prs.slide_layouts[6])
    tb = s2.shapes.add_textbox(Pt(60), Pt(30), Pt(600), Pt(60))
    tf = tb.text_frame
    tf.word_wrap = True
    tf.vertical_anchor = MSO_ANCHOR.MIDDLE
    p = tf.paragraphs[0]
    p.alignment = PP_ALIGN.RIGHT
    p.line_spacing = 1.5
    r = p.add_run()
    r.text = '사회 홍길동 사관'
    r.font.size = Pt(32)
    r.font.bold = True
    r.font.italic = True
    r.font.name = '맑은 고딕'
    r.font.color.rgb = RGBColor(0xFF, 0x33, 0x00)
    p2 = tf.add_paragraph()
    p2.alignment = PP_ALIGN.RIGHT
    r2 = p2.add_run()
    r2.text = '둘째 줄'
    r2.font.size = Pt(18)
    pic = s2.shapes.add_picture(io.BytesIO(png(64, 48, (0x20, 0x80, 0xE0))), Pt(60), Pt(120), Pt(240), Pt(180))
    pic.crop_left = 0.25
    pic.crop_top = 0.1
    rr = s2.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, Pt(360), Pt(120), Pt(240), Pt(80))
    rr.fill.solid()
    rr.fill.fore_color.rgb = RGBColor(0x1D, 0x35, 0x57)
    rr.line.fill.background()
    rr.text_frame.text = '광고'
    rr.text_frame.paragraphs[0].runs[0].font.size = Pt(24)
    rr.text_frame.paragraphs[0].runs[0].font.color.rgb = RGBColor(0xFF, 0xFF, 0xFF)
    el = s2.shapes.add_shape(MSO_SHAPE.OVAL, Pt(660), Pt(120), Pt(120), Pt(120))
    el.fill.solid()
    el.fill.fore_color.theme_color = MSO_THEME_COLOR.ACCENT_1  # 테마 색 (기본 테마 accent1 = 4472C4)
    el.line.fill.background()
    ln = s2.shapes.add_connector(MSO_CONNECTOR.STRAIGHT, Pt(60), Pt(330), Pt(900), Pt(330))
    ln.line.color.rgb = RGBColor(0x88, 0x88, 0x88)
    ln.line.width = Pt(3)
    tbl = s2.shapes.add_table(2, 2, Pt(60), Pt(360), Pt(400), Pt(100)).table
    tbl.cell(0, 0).text = '순서'
    tbl.cell(0, 1).text = '담당'
    tbl.cell(1, 0).text = '기도'
    tbl.cell(1, 1).text = '김철수'
    arrow = s2.shapes.add_shape(MSO_SHAPE.RIGHT_ARROW, Pt(600), Pt(380), Pt(200), Pt(80))
    arrow.fill.solid()
    arrow.fill.fore_color.rgb = RGBColor(0xE9, 0xC4, 0x6A)

    # ③ 숨긴 슬라이드 (내보내지 않는다)
    s3 = prs.slides.add_slide(prs.slide_layouts[6])
    s3.shapes.add_textbox(Pt(0), Pt(0), Pt(300), Pt(50)).text_frame.text = '숨김'
    s3._element.set('show', '0')

    # ④ 단색 배경 + 그룹(사각형 둘) + 글머리표 글
    s4 = prs.slides.add_slide(prs.slide_layouts[6])
    s4.background.fill.solid()
    s4.background.fill.fore_color.rgb = RGBColor(0x10, 0x20, 0x30)
    grp = s4.shapes.add_group_shape()
    a = grp.shapes.add_shape(MSO_SHAPE.RECTANGLE, Pt(100), Pt(100), Pt(200), Pt(100))
    a.fill.solid(); a.fill.fore_color.rgb = RGBColor(0xAA, 0x00, 0x00); a.line.fill.background()
    b = grp.shapes.add_shape(MSO_SHAPE.RECTANGLE, Pt(400), Pt(100), Pt(200), Pt(100))
    b.fill.solid(); b.fill.fore_color.rgb = RGBColor(0x00, 0xAA, 0x00); b.line.fill.background()
    tb4 = s4.shapes.add_textbox(Pt(100), Pt(260), Pt(700), Pt(200))
    tf4 = tb4.text_frame
    tf4.text = '첫째 기도 제목'
    q = tf4.add_paragraph(); q.text = '둘째 기도 제목'
    for para in tf4.paragraphs:
        pPr = para._p.get_or_add_pPr()
        bu = pPr.makeelement('{http://schemas.openxmlformats.org/drawingml/2006/main}buChar', {'char': '•'})
        pPr.append(bu)
        for rn in para.runs:
            rn.font.size = Pt(28)
            rn.font.color.rgb = RGBColor(0xFF, 0xFF, 0xFF)

    # ⑤ 그림 배경(슬라이드 가득한 그림 도형) + 글
    s5 = prs.slides.add_slide(prs.slide_layouts[6])
    s5.shapes.add_picture(io.BytesIO(png(32, 18, (0x30, 0x30, 0x60))), 0, 0, prs.slide_width, prs.slide_height)
    t5 = s5.shapes.add_textbox(Pt(80), Pt(200), Pt(800), Pt(140))
    t5.text_frame.text = '주님은 나의 목자시니'
    t5.text_frame.paragraphs[0].alignment = PP_ALIGN.CENTER
    t5.text_frame.paragraphs[0].runs[0].font.size = Pt(54)
    t5.text_frame.paragraphs[0].runs[0].font.color.rgb = RGBColor(0xFF, 0xFF, 0xFF)

    out = os.path.join(HERE, 'elements.pptx')
    prs.save(out)
    print(out, os.path.getsize(out), 'bytes')


def chart_table():
    """둘째 파일 chart-table.pptx (이음오피스 엔진 점검): ① 막대·원 차트 + 자동 번호 목록 ② 표(기본 스타일) + 길어서 자동 줄바꿈되는 글 + 그라데이션 도형(글 포함) + 돌린 글상자"""
    from pptx.chart.data import CategoryChartData
    from pptx.enum.chart import XL_CHART_TYPE, XL_LEGEND_POSITION
    from pptx.oxml.ns import qn
    prs = Presentation()
    prs.slide_width = Emu(12192000)
    prs.slide_height = Emu(6858000)
    s1 = prs.slides.add_slide(prs.slide_layouts[6])
    cd = CategoryChartData()
    cd.categories = ['1부', '2부', '청년']
    cd.add_series('출석', (120, 85, 40))
    gf = s1.shapes.add_chart(XL_CHART_TYPE.COLUMN_CLUSTERED, Pt(40), Pt(40), Pt(440), Pt(300), cd)
    gf.chart.has_title = True
    gf.chart.chart_title.text_frame.text = '○○교회 출석'
    cd2 = CategoryChartData()
    cd2.categories = ['헌금', '선교', '구제']
    cd2.add_series('비율', (60, 25, 15))
    gf2 = s1.shapes.add_chart(XL_CHART_TYPE.PIE, Pt(500), Pt(40), Pt(420), Pt(300), cd2)
    gf2.chart.has_legend = True
    gf2.chart.legend.position = XL_LEGEND_POSITION.BOTTOM
    tb = s1.shapes.add_textbox(Pt(40), Pt(360), Pt(880), Pt(160))
    tf = tb.text_frame
    tf.word_wrap = True
    for i, t in enumerate(['예배로의 부름', '대표 기도 — 홍길동 사관', '설교 “선한 목자”']):
        p = tf.paragraphs[0] if i == 0 else tf.add_paragraph()
        p.text = t
        pPr = p._p.get_or_add_pPr()
        pPr.set('marL', '342900')
        pPr.set('indent', '-342900')
        pPr.append(pPr.makeelement(qn('a:buAutoNum'), {'type': 'arabicPeriod'}))
        for r in p.runs:
            r.font.size = Pt(24)
    s2 = prs.slides.add_slide(prs.slide_layouts[6])
    tbl = s2.shapes.add_table(3, 3, Pt(40), Pt(40), Pt(500), Pt(150)).table
    for r, row in enumerate([['순서', '담당', '비고'], ['기도', '김철수', ''], ['찬양', '홍길동', '새찬송가 8장']]):
        for c, v in enumerate(row):
            tbl.cell(r, c).text = v
    long = s2.shapes.add_textbox(Pt(560), Pt(40), Pt(360), Pt(300))
    lf = long.text_frame
    lf.word_wrap = True
    lf.text = '하나님이 세상을 이처럼 사랑하사 독생자를 주셨으니 이는 그를 믿는 자마다 멸망하지 않고 영생을 얻게 하려 하심이라'
    lf.paragraphs[0].runs[0].font.size = Pt(28)
    lf.paragraphs[0].runs[0].font.name = '맑은 고딕'
    gr = s2.shapes.add_shape(MSO_SHAPE.RECTANGLE, Pt(40), Pt(220), Pt(500), Pt(120))
    gr.fill.gradient()
    gr.fill.gradient_angle = 90
    gr.fill.gradient_stops[0].color.rgb = RGBColor(0x1D, 0x35, 0x57)
    gr.fill.gradient_stops[1].color.rgb = RGBColor(0xE9, 0xC4, 0x6A)
    gr.text_frame.text = '광고'
    gr.text_frame.paragraphs[0].alignment = PP_ALIGN.CENTER
    gr.text_frame.paragraphs[0].runs[0].font.size = Pt(32)
    gr.text_frame.paragraphs[0].runs[0].font.color.rgb = RGBColor(0xFF, 0xFF, 0xFF)
    rot = s2.shapes.add_textbox(Pt(600), Pt(380), Pt(300), Pt(60))
    rot.rotation = 15
    rot.text_frame.text = '돌린 글'
    out = os.path.join(HERE, 'chart-table.pptx')
    prs.save(out)
    print(out, os.path.getsize(out), 'bytes')


if __name__ == '__main__':
    main()
    chart_table()
